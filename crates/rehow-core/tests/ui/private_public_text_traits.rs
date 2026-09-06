fn needs_ord<T: Ord>() {}
fn needs_partial_ord<T: PartialOrd>() {}
fn needs_hash<T: std::hash::Hash>() {}

fn main() {
    needs_ord::<rehow_core::protocol::PublicText>();
    needs_partial_ord::<rehow_core::protocol::PublicText>();
    needs_hash::<rehow_core::protocol::PublicText>();
}
