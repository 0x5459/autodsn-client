use sc_chain_spec::GenericChainSpec;
use sc_subspace_chain_specs::{DEVNET_CHAIN_SPEC, MAINNET_CHAIN_SPEC, TAURUS_CHAIN_SPEC};

pub fn mainnet_config() -> Result<GenericChainSpec, String> {
    GenericChainSpec::from_json_bytes(MAINNET_CHAIN_SPEC.as_bytes())
}

pub fn taurus_config() -> Result<GenericChainSpec, String> {
    GenericChainSpec::from_json_bytes(TAURUS_CHAIN_SPEC.as_bytes())
}

pub fn devnet_config() -> Result<GenericChainSpec, String> {
    GenericChainSpec::from_json_bytes(DEVNET_CHAIN_SPEC.as_bytes())
}
