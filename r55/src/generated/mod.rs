//! This module contains auto-generated code.
//! Do not edit manually!

use alloy_core::primitives::Bytes;
use core::include_bytes;

pub const HYDRA_BRIDGED_ERC20_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-bridged-erc20.bin");
pub const ERC721_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc721.bin");
pub const DELEGATECALL_IMPL_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/delegatecall-impl.bin");
pub const ERC1967_PROXY_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc1967-proxy.bin");
pub const HYDRA_SIGNAL_SERVICE_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-signal-service.bin");
pub const EVM_CALLER_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/evm-caller.bin");
pub const ERC20_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc20.bin");
pub const HYDRA_ERC20_BRIDGE_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-erc20-bridge.bin");
pub const ERC20X_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/erc20x.bin");
pub const HYDRA_ERC721_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-erc721.bin");
pub const HYDRA_L1_STATE_ROOT_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-l1-state-root.bin");
pub const HYDRA_MESSAGE_RELAYER_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-message-relayer.bin");
pub const HYDRA_ERC721_BRIDGE_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-erc721-bridge.bin");
pub const HYDRA_ERC20_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-erc20.bin");
pub const SIMPLE_DEPOSIT_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/simple-deposit.bin");
pub const NESTED_CALL_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/nested-call.bin");
pub const HYDRA_L2_BRIDGE_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-l2-bridge.bin");
pub const HYDRA_BRIDGED_ERC721_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/hydra-bridged-erc721.bin");
pub const DELEGATECALL_TARGET_BYTECODE: &[u8] = include_bytes!("../../../r55-output-bytecode/delegatecall-target.bin");

pub fn get_bytecode(contract_name: &str) -> Bytes {
    let initcode = match contract_name {
        "hydra_bridged_erc20" => HYDRA_BRIDGED_ERC20_BYTECODE,
        "erc721" => ERC721_BYTECODE,
        "delegatecall_impl" => DELEGATECALL_IMPL_BYTECODE,
        "erc1967_proxy" => ERC1967_PROXY_BYTECODE,
        "hydra_signal_service" => HYDRA_SIGNAL_SERVICE_BYTECODE,
        "evm_caller" => EVM_CALLER_BYTECODE,
        "erc20" => ERC20_BYTECODE,
        "hydra_erc20_bridge" => HYDRA_ERC20_BRIDGE_BYTECODE,
        "erc20x" => ERC20X_BYTECODE,
        "hydra_erc721" => HYDRA_ERC721_BYTECODE,
        "hydra_l1_state_root" => HYDRA_L1_STATE_ROOT_BYTECODE,
        "hydra_message_relayer" => HYDRA_MESSAGE_RELAYER_BYTECODE,
        "hydra_erc721_bridge" => HYDRA_ERC721_BRIDGE_BYTECODE,
        "hydra_erc20" => HYDRA_ERC20_BYTECODE,
        "simple_deposit" => SIMPLE_DEPOSIT_BYTECODE,
        "nested_call" => NESTED_CALL_BYTECODE,
        "hydra_l2_bridge" => HYDRA_L2_BRIDGE_BYTECODE,
        "hydra_bridged_erc721" => HYDRA_BRIDGED_ERC721_BYTECODE,
        "delegatecall_target" => DELEGATECALL_TARGET_BYTECODE,
        _ => return Bytes::new(),
    };

    Bytes::from(initcode)
}
