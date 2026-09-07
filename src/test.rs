#![cfg(test)]

use super::*;
use errors::EscrowError;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};
use types::EscrowState;

fn create_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    TroitEscrowContractClient<'static>,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1000);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let stranger = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = token_contract.address();

    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register_contract(None, TroitEscrowContract);
    let client = TroitEscrowContractClient::new(&env, &contract_id);

    // Initialize admin
    client.initialize(&admin);

    (
        env,
        admin,
        buyer,
        seller,
        stranger,
        token_id,
        client,
        token_client,
        token_admin_client,
    )
}

#[test]
fn test_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TroitEscrowContract);
    let client = TroitEscrowContractClient::new(&env, &contract_id);

    // Get admin before init fails
    assert_eq!(client.try_get_admin(), Err(Ok(EscrowError::NotInitialized)));

    // Init admin succeeds
    assert!(client.try_initialize(&admin).is_ok());
    assert_eq!(client.get_admin(), admin);

    // Double init fails
    let another_admin = Address::generate(&env);
    assert_eq!(
        client.try_initialize(&another_admin),
        Err(Ok(EscrowError::AlreadyInitialized))
    );

    // Set new admin
    client.set_admin(&another_admin);
    assert_eq!(client.get_admin(), another_admin);
}

#[test]
fn test_create_escrow_success() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, _token_client, _token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    let record = client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);

    assert_eq!(record.escrow_id, escrow_id);
    assert_eq!(record.order_id, order_id);
    assert_eq!(record.buyer, buyer);
    assert_eq!(record.seller, seller);
    assert_eq!(record.amount, amount);
    assert_eq!(record.state, EscrowState::Created);

    // Lookup by escrow ID and order ID
    let fetched = client.get_escrow(&escrow_id);
    assert_eq!(fetched, record);

    let fetched_by_order = client.get_escrow_by_order(&order_id);
    assert_eq!(fetched_by_order, record);
}

#[test]
fn test_create_escrow_validations() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, _token_client, _token_admin) =
        create_test_env();

    // Amount <= 0 fails
    assert_eq!(
        client.try_create_escrow(&1, &100, &buyer, &seller, &token_id, &0),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_eq!(
        client.try_create_escrow(&1, &100, &buyer, &seller, &token_id, &-50),
        Err(Ok(EscrowError::InvalidAmount))
    );

    // Buyer == Seller fails
    assert_eq!(
        client.try_create_escrow(&1, &100, &buyer, &buyer, &token_id, &100),
        Err(Ok(EscrowError::SameBuyerSeller))
    );

    // Successful creation
    client.create_escrow(&1, &100, &buyer, &seller, &token_id, &100);

    // Duplicate escrow ID fails
    assert_eq!(
        client.try_create_escrow(&1, &101, &buyer, &seller, &token_id, &100),
        Err(Ok(EscrowError::AlreadyExists))
    );

    // Duplicate order ID fails
    assert_eq!(
        client.try_create_escrow(&2, &100, &buyer, &seller, &token_id, &100),
        Err(Ok(EscrowError::OrderEscrowExists))
    );
}

#[test]
fn test_fund_escrow_success() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    // Mint tokens to buyer
    token_admin.mint(&buyer, &amount);
    assert_eq!(token_client.balance(&buyer), amount);

    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);

    let funded = client.fund_escrow(&escrow_id);
    assert_eq!(funded.state, EscrowState::Funded);
    assert!(funded.funded_at > 0);

    // Tokens transferred to contract
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&client.address), amount);
}

#[test]
fn test_fund_escrow_failures() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, _token_client, token_admin) =
        create_test_env();

    // Fund non-existent escrow fails
    assert_eq!(client.try_fund_escrow(&999), Err(Ok(EscrowError::NotFound)));

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;
    token_admin.mint(&buyer, &amount);

    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);

    // First funding succeeds
    client.fund_escrow(&escrow_id);

    // Double funding fails
    assert_eq!(
        client.try_fund_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );
}

#[test]
fn test_release_escrow_success() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);
    client.fund_escrow(&escrow_id);

    let released = client.release_escrow(&escrow_id);
    assert_eq!(released.state, EscrowState::Released);
    assert!(released.released_at > 0);

    // Funds transferred to seller
    assert_eq!(token_client.balance(&client.address), 0);
    assert_eq!(token_client.balance(&seller), amount);
}

#[test]
fn test_release_escrow_invariants() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, _token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);

    // Release before funding fails
    assert_eq!(
        client.try_release_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );

    client.fund_escrow(&escrow_id);
    client.release_escrow(&escrow_id);

    // Double release fails
    assert_eq!(
        client.try_release_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );

    // Refund after release fails
    assert_eq!(
        client.try_refund_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );
}

#[test]
fn test_refund_escrow_success() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);
    client.fund_escrow(&escrow_id);

    let refunded = client.refund_escrow(&escrow_id);
    assert_eq!(refunded.state, EscrowState::Refunded);
    assert!(refunded.refunded_at > 0);

    // Funds returned to buyer
    assert_eq!(token_client.balance(&client.address), 0);
    assert_eq!(token_client.balance(&buyer), amount);
}

#[test]
fn test_refund_escrow_invariants() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, _token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);

    // Refund before funding fails
    assert_eq!(
        client.try_refund_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );

    client.fund_escrow(&escrow_id);
    client.refund_escrow(&escrow_id);

    // Double refund fails
    assert_eq!(
        client.try_refund_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );

    // Release after refund fails
    assert_eq!(
        client.try_release_escrow(&escrow_id),
        Err(Ok(EscrowError::InvalidState))
    );
}

#[test]
fn test_dispute_and_resolution() {
    let (_env, _admin, buyer, seller, stranger, token_id, client, token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);
    client.fund_escrow(&escrow_id);

    // Stranger cannot dispute
    assert_eq!(
        client.try_dispute_escrow(&escrow_id, &stranger),
        Err(Ok(EscrowError::Unauthorized))
    );

    // Buyer disputes
    let disputed = client.dispute_escrow(&escrow_id, &buyer);
    assert_eq!(disputed.state, EscrowState::Disputed);

    // Admin resolves dispute in favor of buyer
    let resolved = client.resolve_dispute(&escrow_id, &false);
    assert_eq!(resolved.state, EscrowState::Refunded);
    assert_eq!(token_client.balance(&buyer), amount);
}

#[test]
fn test_dispute_resolution_seller() {
    let (_env, _admin, buyer, seller, _stranger, token_id, client, token_client, token_admin) =
        create_test_env();

    let escrow_id = 1u64;
    let order_id = 100u64;
    let amount = 500i128;

    token_admin.mint(&buyer, &amount);
    client.create_escrow(&escrow_id, &order_id, &buyer, &seller, &token_id, &amount);
    client.fund_escrow(&escrow_id);

    // Seller disputes
    client.dispute_escrow(&escrow_id, &seller);

    // Admin resolves dispute in favor of seller
    let resolved = client.resolve_dispute(&escrow_id, &true);
    assert_eq!(resolved.state, EscrowState::Released);
    assert_eq!(token_client.balance(&seller), amount);
}
