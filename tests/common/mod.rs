//! Shared test helpers for perc5ive integration tests.
//!
//! [`make_account_info`] builds a `five_vm_mito::AccountInfo` (= five-mono's
//! vendored `pinocchio::account_info::AccountInfo`) using that crate's public
//! `AccountInfo::new` test constructor. The dev-dependency is pinned to the
//! same vendored path five-vm-mito links, so the type unifies with the slice
//! `MitoVM::execute_direct` expects. Production Percolator handlers run under a
//! Solana runtime that constructs `AccountInfo` instances for us; this is
//! strictly test scaffolding.

#![allow(dead_code)]

use pinocchio::{account_info::AccountInfo, pubkey::Pubkey};

/// The VM's program id that [`check_bytecode_authorization`] compares each
/// STOREd account's owner against. Mirrors `five_vm_mito::FIVE_VM_PROGRAM_ID`.
/// Copied locally so tests don't pay for a path-dep on a private constant
/// re-export.
pub const FIVE_VM_PROGRAM_ID: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

/// Allocate a fresh, mutable `AccountInfo` via pinocchio's public test
/// constructor. `AccountInfo::new` copies `initial_data` and `lamports` into a
/// heap allocation it owns, so the inputs are passed by `&mut` but only read.
/// The allocation is never freed (fine for per-test scaffolding); the VM may
/// mutate the data region through `STORE_FIELD`.
pub fn make_account_info(
    key: Pubkey,
    owner: Pubkey,
    is_signer: bool,
    is_writable: bool,
    lamports: u64,
    initial_data: Vec<u8>,
) -> AccountInfo {
    // Leak the backing storage so the &mut slice/lamports outlive this call;
    // `AccountInfo::new` copies them into its own allocation, but takes &mut.
    let mut lamports = lamports;
    let data: &'static mut [u8] = Box::leak(initial_data.into_boxed_slice());
    AccountInfo::new(
        &key,
        is_signer,
        is_writable,
        &mut lamports,
        data,
        &owner,
        false, // executable
        0,     // rent_epoch (ignored by the vendored constructor)
    )
}

/// Read the current contents of an AccountInfo's data region as a fresh Vec.
/// Equivalent to `account.borrow_data_unchecked().to_vec()` but without
/// requiring a named borrow lifetime in test code.
pub fn read_data(account: &AccountInfo) -> Vec<u8> {
    unsafe { account.borrow_data_unchecked() }.to_vec()
}

/// Write the 16 little-endian bytes of a u128 into `buf` starting at `offset`.
pub fn put_u128(buf: &mut [u8], offset: usize, value: u128) {
    buf[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
}

/// Write the 16 little-endian bytes of an i128 (two's complement) into `buf`.
pub fn put_i128(buf: &mut [u8], offset: usize, value: i128) {
    buf[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
}

/// Write the 8 little-endian bytes of a u64 into `buf` starting at `offset`.
pub fn put_u64(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Write the 2 little-endian bytes of a u16 into `buf` starting at `offset`.
pub fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn get_u128(buf: &[u8], offset: usize) -> u128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(&buf[offset..offset + 16]);
    u128::from_le_bytes(b)
}

pub fn get_i128(buf: &[u8], offset: usize) -> i128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(&buf[offset..offset + 16]);
    i128::from_le_bytes(b)
}

pub fn get_u64(buf: &[u8], offset: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&buf[offset..offset + 8]);
    u64::from_le_bytes(b)
}

pub fn get_u16(buf: &[u8], offset: usize) -> u16 {
    let mut b = [0u8; 2];
    b.copy_from_slice(&buf[offset..offset + 2]);
    u16::from_le_bytes(b)
}

/// A dummy 32-byte key with a distinct trailing byte so logs can tell them
/// apart. `tag` picks the trailing byte.
pub fn fake_key(tag: u8) -> Pubkey {
    let mut k = [0u8; 32];
    k[31] = tag;
    k
}

/// Convenience: build an AccountInfo owned by the Five VM program and
/// writable, with the given initial data. This is the account shape most
/// Percolator handler tests want — STORE_FIELD runs
/// `check_bytecode_authorization` which requires the owner pubkey to equal
/// the VM's program id.
pub fn make_vm_account(tag: u8, is_signer: bool, initial_data: Vec<u8>) -> AccountInfo {
    make_account_info(
        fake_key(tag),
        FIVE_VM_PROGRAM_ID,
        is_signer,
        true,
        1_000_000,
        initial_data,
    )
}
