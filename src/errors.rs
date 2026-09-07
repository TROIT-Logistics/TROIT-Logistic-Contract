use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    AlreadyExists = 3,
    NotFound = 4,
    InvalidAmount = 5,
    InvalidState = 6,
    Unauthorized = 7,
    SameBuyerSeller = 8,
    OrderEscrowExists = 9,
}
