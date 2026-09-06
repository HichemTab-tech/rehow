use std::str::FromStr;
fn main(){let id=rehow_core::protocol::CategoryId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();let _=rehow_core::protocol::CategoryNotFoundDetails{category_id:id};}
