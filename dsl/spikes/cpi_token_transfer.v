// Phase 0, Spike 1 — confirm the genesis-vault deposit/withdraw CPI mechanic
// is expressible + compiles against the mono DSL compiler.
//
// percolator-meta's genesis_deposit transfers base units user_ata -> vault via
// a plain SPL `transfer` (signed by the user), and genesis_withdraw transfers
// vault -> user_ata via `transfer` signed by the market_admin PDA. Both are the
// same instruction (SPL Token tag 3); the only difference is which authority
// signs. This spike models both shapes.

interface SPLToken @program("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA") {
    transfer @discriminator(3) (source: pubkey, destination: pubkey, authority: pubkey, amount: u64);
}

// Deposit leg: user signs the transfer into the program vault.
pub genesis_deposit_cpi(
    user: account @signer,
    user_ata: account @mut,
    genesis_vault: account @mut,
    amount: u64
) {
    SPLToken::transfer(user_ata, genesis_vault, user, amount);
}

// Withdraw leg: the market_admin PDA signs the transfer back out of the vault.
pub genesis_withdraw_cpi(
    market_admin: account @signer,
    genesis_vault: account @mut,
    user_ata: account @mut,
    amount: u64
) {
    SPLToken::transfer(genesis_vault, user_ata, market_admin, amount);
}
