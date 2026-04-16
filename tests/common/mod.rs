//! Shared test helpers for perc5ive integration tests.
//!
//! The main thing here is [`make_account_info`]: pinocchio 0.9.2's
//! `AccountInfo` has a `pub(crate)` constructor and no public `new`, which
//! makes it impossible to build from outside the crate. Because the struct is
//! `#[repr(C)]` with a single raw pointer field, we mirror its private layout
//! locally and reinterpret-cast the leaked pointer. This is strictly test
//! code — production Percolator handlers run under a Solana runtime that
//! constructs `AccountInfo` instances for us.

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

/// Mirror of pinocchio 0.9.2's private `Account` struct.
///
/// Field order / sizes / alignment must match
/// `pinocchio-0.9.2/src/account_info.rs`'s `#[repr(C)] pub(crate) struct
/// Account`. A regression in pinocchio that changes this layout will manifest
/// as bogus account data in the VM — fail loudly in tests rather than silently
/// corrupt.
#[repr(C)]
struct MirroredAccount {
    borrow_state: u8,
    is_signer: u8,
    is_writable: u8,
    executable: u8,
    resize_delta: i32,
    key: Pubkey,
    owner: Pubkey,
    lamports: u64,
    data_len: u64,
}

const _: () = {
    assert!(core::mem::size_of::<MirroredAccount>() == 88);
};

/// Allocate a fresh, mutable `AccountInfo` backed by leaked heap storage.
///
/// The returned `AccountInfo` owns no lifetime — the backing `Vec<u8>` is
/// leaked on purpose so the data and the AccountInfo's pointer remain valid
/// for the lifetime of the test process. Memory is not reclaimed; fine for a
/// per-test allocation.
///
/// `initial_data` is copied verbatim into the data region; the VM may mutate
/// it through `STORE_FIELD`.
pub fn make_account_info(
    key: Pubkey,
    owner: Pubkey,
    is_signer: bool,
    is_writable: bool,
    lamports: u64,
    initial_data: Vec<u8>,
) -> AccountInfo {
    let data_len = initial_data.len();
    let total_len = core::mem::size_of::<MirroredAccount>() + data_len;

    // Box::leak forever. Reallocating as a Vec<u8> keeps alignment guaranteed
    // by Rust's allocator, which is >= 8 for this size — more than enough for
    // MirroredAccount (max alignment 4 due to i32, but access through data_ptr
    // reads u8s so alignment doesn't matter).
    let mut buf = vec![0u8; total_len].into_boxed_slice();

    let header = MirroredAccount {
        borrow_state: 0xFF, // all borrows available — see pinocchio borrow_state bit layout
        is_signer: u8::from(is_signer),
        is_writable: u8::from(is_writable),
        executable: 0,
        resize_delta: 0,
        key,
        owner,
        lamports,
        data_len: data_len as u64,
    };
    unsafe {
        let header_ptr = buf.as_mut_ptr() as *mut MirroredAccount;
        core::ptr::write(header_ptr, header);
        core::ptr::copy_nonoverlapping(
            initial_data.as_ptr(),
            buf.as_mut_ptr().add(core::mem::size_of::<MirroredAccount>()),
            data_len,
        );
    }

    let leaked = Box::leak(buf);
    let raw_ptr = leaked.as_mut_ptr();

    // `AccountInfo` is `#[repr(C)] { raw: *mut Account }` — a single pointer.
    // Transmute the raw buffer pointer through it. The buffer starts with a
    // MirroredAccount header whose layout matches pinocchio's internal
    // Account, so dereferencing `(*self.raw).field` inside the pinocchio
    // crate will read the correct bytes.
    unsafe { core::mem::transmute::<*mut u8, AccountInfo>(raw_ptr) }
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
