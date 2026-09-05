use rehow_core::protocol::ContractGenerationSmoke;
use ts_rs::{Config, TS};

fn main() {
    println!(
        "// This file is generated. Do not edit.\nexport {}",
        ContractGenerationSmoke::decl(&Config::default())
    );
}
