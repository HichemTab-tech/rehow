fn main() {
    let _ = rehow_core::protocol::WorkspaceId::new_v7();
    let _ = rehow_core::protocol::Timestamp::now_utc();
    let _ = rehow_core::protocol::PlanToken::generate();
    let _ = rehow_core::protocol::Revision::from_digest([0_u8; 32]);
}
