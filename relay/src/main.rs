//! The blind relay: stage 3.
//!
//! Why it exists: if the recipient is offline, someone who is online must accept the
//! message. This role is physically irremovable (§2.1 of the research), and Briar, SimpleX
//! and Keet independently arrived at the same minimum: a node that stores ciphertext and
//! cannot read it (§13.3). It is not an operator's server: anyone can run one, and
//! in the client the list is replaceable.
//!
//! What it will be able to do and, more importantly, what it will not:
//!   * one-way queues, a separate set for each contact;
//!   * the sending address is not equal to the receiving address;
//!   * no records about users and no global identifiers;
//!   * deletion of a message after delivery;
//!   * fixed-size envelopes: otherwise the length itself becomes metadata.
//!
//! Implementation: stage 3 of the plan. For now it is a stub that holds a place in the workspace.

fn main() {
    println!(
        "apeiron-relay {}: a stub. Implementation in stage 3, protocol v{}.",
        env!("CARGO_PKG_VERSION"),
        apeiron_core::PROTOCOL_VERSION
    );
}
