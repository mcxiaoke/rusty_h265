//! CABAC steganography observer hooks.
//!
//! This is a port of the `CabacObserver` contract from the `rust_h265` fork
//! used by the strhide project. The trait lets an external recorder watch the
//! arithmetic decode of a slice segment bin by bin, isolate the bypass sign
//! bits that carry coefficient signs, and capture WPP/tile substream
//! boundaries so a CABAC *encoder* can replay the very same bin stream.
//!
//! # Contract
//!
//! - Hooks fire in decode order, from a single thread, for the duration of
//!   one [`crate::Decoder::decode_nal_with_observer`] call.
//! - `record_stego_sign` and `record_stego_level` pair up per coefficient:
//!   signs are parsed before their levels, so the sign callback stores a
//!   provisional cost and the level callback back-fills the real magnitude
//!   in the same coding-group order. Both skip the DC `(0, 0)` coefficient
//!   on the same coordinate test, which keeps a plain cursor aligned.
//! - `start_substream(0, ..)` fires once per slice segment with the initial
//!   context state; one more per WPP row / tile entry point, with the
//!   context state that substream actually starts from.
//!
//! # Numbering
//!
//! `ctx_idx` values follow this crate's **native** numbering — the compact
//! 157-slot layout of [`crate::cabac::NUM_CTX`], not the 179-slot layout of
//! the `rust_h265` fork / FFmpeg. The two are semantically aligned but
//! numbered differently.
//!
//! A CABAC encoder does not care *which* numbering it is handed: it
//! initialises its model array from [`ContextSnapshot`] and then encodes
//! every bin at the index it is told. So the contract that makes a journal
//! replayable is "indices and snapshot use the same layout", not "indices
//! match some other decoder's layout". This crate guarantees that by
//! widening the native snapshot into [`ContextSnapshot`]: the native states
//! occupy slots `0..NUM_CTX`, and the slots above are filler that no bin
//! ever references.
//!
//! [`ContextSnapshot`] is intentionally sized to [`ENCODER_CONTEXTS`] (179,
//! the `rust_h265` fork's `HEVC_CONTEXTS`) so a recorder written against
//! that fork's `CabacObserver` can implement this trait without changing a
//! single method body.

use crate::cabac::{Contexts, NUM_CTX};

/// Context slots in the `rust_h265` fork / FFmpeg layout.
///
/// Native indices are all `< NUM_CTX < ENCODER_CONTEXTS`, so a recorder that
/// asserts `ctx_idx < ENCODER_CONTEXTS` (as the strhide CABAC encoder does)
/// accepts every index this crate reports.
pub const ENCODER_CONTEXTS: usize = 179;

/// Context state at a substream start, in the encoder's slot layout.
pub type ContextSnapshot = [u8; ENCODER_CONTEXTS];

/// Widens the native context state into a [`ContextSnapshot`].
///
/// Slots `0..NUM_CTX` carry the live models; the remainder is filler, never
/// read by a replay because no recorded index reaches it.
#[inline]
pub fn context_snapshot(ctx: &Contexts) -> ContextSnapshot {
    let mut out = [0u8; ENCODER_CONTEXTS];
    out[..NUM_CTX].copy_from_slice(&ctx.0[..NUM_CTX]);
    out
}

pub trait CabacObserver {
    /// Marks the beginning of a new substream within a slice segment.
    ///
    /// `rbsp_byte_start` is the byte offset of the substream's first CABAC
    /// byte in the slice's RBSP; `init_contexts` is the context state the
    /// substream decodes from, widened per [`context_snapshot`].
    fn start_substream(&mut self, substream_idx: usize, rbsp_byte_start: usize, init_contexts: &ContextSnapshot);

    /// Marks the end of the slice segment data (`rbsp_len` = RBSP length).
    fn finish_slice(&mut self, rbsp_len: usize);

    /// Records a context-coded decision bin.
    fn record_decision(&mut self, ctx_idx: usize, bit: u32);

    /// Records an equiprobable bypass bin (non-sign).
    fn record_bypass(&mut self, bit: u32);

    /// Records a bypass sign bit as a candidate steganographic slot.
    fn record_stego_sign(&mut self, bit: u32, qp: i32, trafo_size: u8, c_idx: u8, x_c: u8, y_c: u8);

    /// Back-fills the coefficient magnitude for the sign just recorded by
    /// [`CabacObserver::record_stego_sign`].
    ///
    /// Calls arrive in the same order as the matching sign records, DC
    /// `(0, 0)` excluded on both sides. `abs_dq` is the dequantized value
    /// that reaches the inverse transform (spec 8.6.3 `bdShift` applied).
    #[inline]
    fn record_stego_level(&mut self, _x_c: u8, _y_c: u8, _abs_level: u32, _abs_dq: i32, _bit_depth: u8) {}

    /// Records a terminate bin.
    fn record_terminate(&mut self, bit: u32);
}
