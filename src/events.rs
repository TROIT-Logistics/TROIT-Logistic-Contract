use soroban_sdk::{symbol_short, Address, Env};

pub fn emit_created(
    env: &Env,
    escrow_id: u64,
    order_id: u64,
    buyer: &Address,
    seller: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("created"), escrow_id, order_id),
        (buyer.clone(), seller.clone(), amount),
    );
}

pub fn emit_funded(env: &Env, escrow_id: u64, buyer: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("funded"), escrow_id),
        (buyer.clone(), amount),
    );
}

pub fn emit_released(env: &Env, escrow_id: u64, seller: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("released"), escrow_id),
        (seller.clone(), amount),
    );
}

pub fn emit_refunded(env: &Env, escrow_id: u64, buyer: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("refunded"), escrow_id),
        (buyer.clone(), amount),
    );
}

pub fn emit_disputed(env: &Env, escrow_id: u64, caller: &Address) {
    env.events()
        .publish((symbol_short!("disputed"), escrow_id), caller.clone());
}
