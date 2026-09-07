#![no_std]

pub mod errors;
pub mod events;
pub mod types;

use errors::EscrowError;
use soroban_sdk::{contract, contractimpl, token, Address, Env};
use types::{DataKey, EscrowRecord, EscrowState};

#[contract]
pub struct TroitEscrowContract;

#[contractimpl]
impl TroitEscrowContract {
    /// Initialize contract admin. Can only be called once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            return Err(EscrowError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Get current admin address.
    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(EscrowError::NotInitialized)
    }

    /// Update contract admin address. Requires current admin authorization.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), EscrowError> {
        let admin = Self::get_admin(env.clone())?;
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Create a new escrow record for an order.
    /// State starts at `Created`. Must be funded before release or refund.
    pub fn create_escrow(
        env: Env,
        escrow_id: u64,
        order_id: u64,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
    ) -> Result<EscrowRecord, EscrowError> {
        buyer.require_auth();

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        if buyer == seller {
            return Err(EscrowError::SameBuyerSeller);
        }

        if env.storage().persistent().has(&DataKey::Escrow(escrow_id)) {
            return Err(EscrowError::AlreadyExists);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::OrderEscrow(order_id))
        {
            return Err(EscrowError::OrderEscrowExists);
        }

        let timestamp = env.ledger().timestamp();
        let record = EscrowRecord {
            escrow_id,
            order_id,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            state: EscrowState::Created,
            created_at: timestamp,
            funded_at: 0,
            released_at: 0,
            refunded_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::OrderEscrow(order_id), &escrow_id);

        events::emit_created(&env, escrow_id, order_id, &buyer, &seller, amount);

        Ok(record)
    }

    /// Fund an existing escrow by transferring tokens from buyer to contract.
    /// Requirements:
    /// - Escrow must exist in `Created` state.
    /// - Caller must be the authorized buyer.
    /// - Tokens transferred equal exact escrow amount.
    pub fn fund_escrow(env: Env, escrow_id: u64) -> Result<EscrowRecord, EscrowError> {
        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)?;

        if escrow.state != EscrowState::Created {
            return Err(EscrowError::InvalidState);
        }

        escrow.buyer.require_auth();

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &escrow.buyer,
            &env.current_contract_address(),
            &escrow.amount,
        );

        let timestamp = env.ledger().timestamp();
        escrow.state = EscrowState::Funded;
        escrow.funded_at = timestamp;

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        events::emit_funded(&env, escrow_id, &escrow.buyer, escrow.amount);

        Ok(escrow)
    }

    /// Release escrowed funds to seller after buyer delivery confirmation.
    /// Requirements:
    /// - Escrow must be in `Funded` or `Disputed` state.
    /// - Buyer authorization required if `Funded`. Admin authorization if `Disputed`.
    /// - Cannot release twice or release refunded escrow.
    pub fn release_escrow(env: Env, escrow_id: u64) -> Result<EscrowRecord, EscrowError> {
        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)?;

        match escrow.state {
            EscrowState::Funded => {
                escrow.buyer.require_auth();
            }
            EscrowState::Disputed => {
                let admin = Self::get_admin(env.clone())?;
                admin.require_auth();
            }
            _ => return Err(EscrowError::InvalidState),
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        let timestamp = env.ledger().timestamp();
        escrow.state = EscrowState::Released;
        escrow.released_at = timestamp;

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        events::emit_released(&env, escrow_id, &escrow.seller, escrow.amount);

        Ok(escrow)
    }

    /// Refund escrowed funds back to buyer.
    /// Requirements:
    /// - Escrow must be in `Funded` or `Disputed` state.
    /// - Seller authorization required if `Funded` (voluntary refund). Admin if `Disputed`.
    /// - Cannot refund twice or refund released escrow.
    pub fn refund_escrow(env: Env, escrow_id: u64) -> Result<EscrowRecord, EscrowError> {
        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)?;

        match escrow.state {
            EscrowState::Funded => {
                escrow.seller.require_auth();
            }
            EscrowState::Disputed => {
                let admin = Self::get_admin(env.clone())?;
                admin.require_auth();
            }
            _ => return Err(EscrowError::InvalidState),
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        let timestamp = env.ledger().timestamp();
        escrow.state = EscrowState::Refunded;
        escrow.refunded_at = timestamp;

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        events::emit_refunded(&env, escrow_id, &escrow.buyer, escrow.amount);

        Ok(escrow)
    }

    /// Flag an escrow as disputed by either buyer or seller.
    /// Requirements:
    /// - Escrow must be in `Funded` state.
    /// - Caller must be either buyer or seller.
    pub fn dispute_escrow(
        env: Env,
        escrow_id: u64,
        caller: Address,
    ) -> Result<EscrowRecord, EscrowError> {
        caller.require_auth();

        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)?;

        if caller != escrow.buyer && caller != escrow.seller {
            return Err(EscrowError::Unauthorized);
        }

        if escrow.state != EscrowState::Funded {
            return Err(EscrowError::InvalidState);
        }

        escrow.state = EscrowState::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        events::emit_disputed(&env, escrow_id, &caller);

        Ok(escrow)
    }

    /// Admin resolution for disputed escrows.
    /// Directs funds to seller (if `release_to_seller` is true) or buyer (if false).
    pub fn resolve_dispute(
        env: Env,
        escrow_id: u64,
        release_to_seller: bool,
    ) -> Result<EscrowRecord, EscrowError> {
        let admin = Self::get_admin(env.clone())?;
        admin.require_auth();

        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)?;

        if escrow.state != EscrowState::Disputed {
            return Err(EscrowError::InvalidState);
        }

        let token_client = token::Client::new(&env, &escrow.token);
        let timestamp = env.ledger().timestamp();

        if release_to_seller {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.seller,
                &escrow.amount,
            );
            escrow.state = EscrowState::Released;
            escrow.released_at = timestamp;
            events::emit_released(&env, escrow_id, &escrow.seller, escrow.amount);
        } else {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.buyer,
                &escrow.amount,
            );
            escrow.state = EscrowState::Refunded;
            escrow.refunded_at = timestamp;
            events::emit_refunded(&env, escrow_id, &escrow.buyer, escrow.amount);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        Ok(escrow)
    }

    /// Retrieve escrow record by escrow ID.
    pub fn get_escrow(env: Env, escrow_id: u64) -> Result<EscrowRecord, EscrowError> {
        env.storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .ok_or(EscrowError::NotFound)
    }

    /// Retrieve escrow record by order ID reference.
    pub fn get_escrow_by_order(env: Env, order_id: u64) -> Result<EscrowRecord, EscrowError> {
        let escrow_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::OrderEscrow(order_id))
            .ok_or(EscrowError::NotFound)?;

        Self::get_escrow(env, escrow_id)
    }
}

mod test;
