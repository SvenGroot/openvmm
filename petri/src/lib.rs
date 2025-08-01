// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A Rust-based testing framework for VMMs.
//!
//! At this time - `petri` supports testing OpenVMM, OpenHCL,
//! and Hyper-V based VMs.

#![forbid(unsafe_code)]

pub mod disk_image;
mod linux_direct_serial_agent;
// TODO: Add docs and maybe a trait interface for this, or maybe this can
// remain crate-local somehow without violating interface privacy.
#[expect(missing_docs)]
pub mod openhcl_diag;
mod test;
mod tracing;
mod vm;
mod worker;

pub use petri_artifacts_core::ArtifactHandle;
pub use petri_artifacts_core::ArtifactResolver;
pub use petri_artifacts_core::AsArtifactHandle;
pub use petri_artifacts_core::ErasedArtifactHandle;
pub use petri_artifacts_core::ResolveTestArtifact;
pub use petri_artifacts_core::ResolvedArtifact;
pub use petri_artifacts_core::ResolvedOptionalArtifact;
pub use petri_artifacts_core::TestArtifactRequirements;
pub use petri_artifacts_core::TestArtifacts;
pub use pipette_client as pipette;
pub use test::PetriTestParams;
pub use test::RunTest;
pub use test::SimpleTest;
pub use test::TestCase;
pub use test::test_macro_support;
pub use test::test_main;
pub use tracing::*;
pub use vm::*;

/// 1 kibibyte's worth of bytes.
pub const SIZE_1_KB: u64 = 1024;
/// 1 mebibyte's worth of bytes.
pub const SIZE_1_MB: u64 = 1024 * SIZE_1_KB;
/// 1 gibibyte's worth of bytes.
pub const SIZE_1_GB: u64 = 1024 * SIZE_1_MB;

/// The kind of shutdown to perform.
#[expect(missing_docs)] // Self-describing names.
pub enum ShutdownKind {
    Shutdown,
    Reboot,
    // TODO: Add hibernate?
}
