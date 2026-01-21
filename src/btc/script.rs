use bitcoin::{
    ScriptBuf, XOnlyPublicKey,
    opcodes::{OP_FALSE, all::*},
    script::Builder,
};

use crate::deposit::CitreaAddress;

pub(crate) fn recover_script(
    recovery_taproot_address: XOnlyPublicKey,
    timelock_amount: u64,
) -> ScriptBuf {
    Builder::new()
        .push_int(timelock_amount as i64)
        .push_opcode(OP_CSV)
        .push_opcode(OP_DROP)
        .push_x_only_key(&recovery_taproot_address)
        .push_opcode(OP_CHECKSIG)
        .into_script()
}

pub(crate) fn deposit_script(
    citrea_address: CitreaAddress,
    nofn_xonly_pk: XOnlyPublicKey,
) -> ScriptBuf {
    let citrea: [u8; 6] = "citrea".as_bytes().try_into().expect("length == 6");

    Builder::new()
        .push_x_only_key(&nofn_xonly_pk)
        .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_FALSE)
        .push_opcode(OP_IF)
        .push_slice(citrea)
        .push_slice(*citrea_address.0)
        .push_opcode(OP_ENDIF)
        .into_script()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::opcodes::all::{OP_CHECKSIG, OP_CSV, OP_DROP, OP_ENDIF, OP_IF};
    use bitcoin::script::Instruction;
    use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};

    fn sample_xonly_key() -> XOnlyPublicKey {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[7u8; 32]).expect("secret key");
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let (xonly, _) = XOnlyPublicKey::from_keypair(&keypair);
        xonly
    }

    #[test]
    fn recover_script_pushes_timelock_and_key_in_order() {
        let xonly = sample_xonly_key();
        let timelock = 144u64;
        let script = recover_script(xonly, timelock);
        let instructions: Vec<Instruction> = script
            .as_script()
            .instructions()
            .map(|ins| ins.expect("instruction"))
            .collect();

        assert_eq!(instructions.len(), 5);
        assert_eq!(instructions[0].script_num(), Some(timelock as i64));
        assert_eq!(instructions[1], Instruction::Op(OP_CSV));
        assert_eq!(instructions[2], Instruction::Op(OP_DROP));

        match &instructions[3] {
            Instruction::PushBytes(bytes) => {
                assert_eq!(bytes.as_bytes(), &xonly.serialize()[..]);
            }
            _ => panic!("expected x-only pubkey push"),
        }

        assert_eq!(instructions[4], Instruction::Op(OP_CHECKSIG));
    }

    #[test]
    fn deposit_script_embeds_tag_and_address() {
        let xonly = sample_xonly_key();
        let citrea_address = CitreaAddress::from([0x11; 20]);
        let script = deposit_script(citrea_address, xonly);
        let instructions: Vec<Instruction> = script
            .as_script()
            .instructions()
            .map(|ins| ins.expect("instruction"))
            .collect();

        assert_eq!(instructions.len(), 7);

        match &instructions[0] {
            Instruction::PushBytes(bytes) => {
                assert_eq!(bytes.as_bytes(), &xonly.serialize()[..]);
            }
            _ => panic!("expected x-only pubkey push"),
        }

        assert_eq!(instructions[1], Instruction::Op(OP_CHECKSIG));

        match &instructions[2] {
            Instruction::PushBytes(bytes) => assert!(bytes.as_bytes().is_empty()),
            _ => panic!("expected OP_FALSE push"),
        }

        assert_eq!(instructions[3], Instruction::Op(OP_IF));

        match &instructions[4] {
            Instruction::PushBytes(bytes) => assert_eq!(bytes.as_bytes(), b"citrea"),
            _ => panic!("expected citrea tag push"),
        }

        let expected_citrea_bytes = *citrea_address.0;
        match &instructions[5] {
            Instruction::PushBytes(bytes) => {
                assert_eq!(bytes.as_bytes(), &expected_citrea_bytes[..]);
            }
            _ => panic!("expected citrea address push"),
        }

        assert_eq!(instructions[6], Instruction::Op(OP_ENDIF));
    }
}
