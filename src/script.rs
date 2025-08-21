use bitcoin::{
    ScriptBuf, XOnlyPublicKey,
    opcodes::{OP_FALSE, all::*},
    script::{Builder, PushBytesBuf},
};

use crate::CitreaAddress;

pub(crate) fn recover_script(recovery_taproot_address: XOnlyPublicKey, timelock_amount: u64) -> ScriptBuf {
    Builder::new()
        .push_int(timelock_amount as i64)
        .push_opcode(OP_CSV)
        .push_opcode(OP_DROP)
        .push_x_only_key(&recovery_taproot_address)
        .push_opcode(OP_CHECKSIG)
        .into_script()
}

pub(crate) fn deposit_script(citrea_address: CitreaAddress, nofn_xonly_pk: XOnlyPublicKey) -> ScriptBuf {
    let citrea: [u8; 6] = "citrea".as_bytes().try_into().expect("length == 6");

    Builder::new()
        .push_x_only_key(&nofn_xonly_pk)
        .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_FALSE)
        .push_opcode(OP_IF)
        .push_slice(citrea)
        .push_slice(*citrea_address.0)
        .push_slice(PushBytesBuf::try_from(hex::decode("000000003b9aca00").unwrap()).unwrap())
        .push_opcode(OP_ENDIF)
        .into_script()
}
