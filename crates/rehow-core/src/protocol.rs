use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Non-product type used only to prove deterministic contract generation.
#[derive(Debug, Deserialize, Serialize, TS)]
pub struct ContractGenerationSmoke {
    pub protocol_version: u32,
}

#[cfg(test)]
mod tests {
    use super::ContractGenerationSmoke;

    #[test]
    fn constructs_contract_generation_smoke() {
        let smoke = ContractGenerationSmoke {
            protocol_version: 1,
        };
        assert_eq!(smoke.protocol_version, 1);
    }
}
