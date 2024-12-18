const { Command } = require('commander')
const { ECPairFactory } = require('ecpair')
const ecc = require('@bitcoinerlab/secp256k1')
const bitcoin = require('bitcoinjs-lib')
bitcoin.initEccLib(ecc)
const ECPair = ECPairFactory(ecc)

const network = bitcoin.networks.testnet
const INTERNAL_PUBKEY_HEX =
  '93c7378d96518a75448821c4f7c8f4bae7ce60f804d03d1f0628dd5dd0f5de51'
const INTERNAL_PUBKEY = Buffer.from(INTERNAL_PUBKEY_HEX, 'hex')
const VERIFIER_PKS =
  '9fb3a961d8b1f4ec1caa220c6a50b815febc0b689ddf0b9ddfbf99cb74479e41'
const BRIDGE_AMOUNT_SATS = 1000000000
const BRIDGE_AMOUNT_SATS_HEX = BRIDGE_AMOUNT_SATS.toString(16).padStart(16, '0')
const USER_TAKES_AFTER = 200
const CITREA = '636974726561'

const sign = (data, privateKey) => {
  const keyPair = ECPair.fromPrivateKey(Buffer.from(privateKey, 'hex'))
  const tweakedSigner = keyPair.tweak(
    bitcoin.crypto.taggedHash('TapTweak', keyPair.publicKey.subarray(1, 33))
  )
  return tweakedSigner.signSchnorr(data)
}

const convertTxToSighash = (tx, prevouts, scripts, txinIndex, scriptIndex) => {
  const prevouts2 = prevouts.map(prevout => ({
    hash: Buffer.from(prevout.txid, 'hex').reverse(),
    index: prevout.vout,
    value: prevout.value,
    script: Buffer.from(prevout.script, 'hex'),
  }))
  const script = Buffer.from(scripts[txinIndex][scriptIndex], 'hex')
  const data = Buffer.concat([
    Buffer.from('c0', 'hex'),
    Buffer.from([script.length]),
    script,
  ])
  return tx.hashForWitnessV1(
    txinIndex,
    prevouts2.map(prev => prev.script),
    prevouts2.map(prev => prev.value),
    bitcoin.Transaction.SIGHASH_DEFAULT,
    bitcoin.crypto.taggedHash('TapLeaf', data)
  )
}

const getMoveScript = evmAddress => {
  return bitcoin.script.fromASM(
    `${VERIFIER_PKS.split(',')
      .flatMap(pk => [pk, 'OP_CHECKSIGVERIFY'])
      .slice(0, -1)
      .join(
        ' '
      )} OP_CHECKSIG OP_0 OP_IF ${CITREA} ${evmAddress} ${BRIDGE_AMOUNT_SATS_HEX} OP_ENDIF`
  )
}

const getUserTakesScript = tweakedPK => {
  return bitcoin.script.fromASM(
    `${USER_TAKES_AFTER.toString(16)
      .padStart(4, '0')
      .match(/.{1,2}/g)
      .reverse()
      .join('')} OP_CHECKSEQUENCEVERIFY OP_DROP ${tweakedPK} OP_CHECKSIG`
  )
}

const prepareUserTakesBackTx = (
  privateKey,
  address,
  evmAddress,
  txid,
  vout,
  amount,
  fee
) => {
  const tweakedPK = bitcoin.address.fromBech32(address).data.toString('hex')
  const userTakesScript = getUserTakesScript(tweakedPK)
  const moveScript = getMoveScript(evmAddress)
  const p2tr = bitcoin.payments.p2tr({
    internalPubkey: INTERNAL_PUBKEY,
    scriptTree: [{ output: moveScript }, { output: userTakesScript }],
    redeem: { output: userTakesScript },
    network: network,
  })
  const userAddrScriptPubKey = Buffer.concat([
    Buffer.from('5120', 'hex'),
    bitcoin.address.fromBech32(address).data,
  ])
  const tx = new bitcoin.Transaction()
  tx.version = 2
  tx.addInput(Buffer.from(txid, 'hex').reverse(), vout, USER_TAKES_AFTER + 1)
  tx.addOutput(userAddrScriptPubKey, amount - fee)
  const prevouts = [
    {
      txid: txid,
      vout: vout,
      value: amount,
      script: p2tr.output,
    },
  ]
  const scripts = [
    [moveScript.toString('hex'), userTakesScript.toString('hex')],
  ]
  const sighash = convertTxToSighash(tx, prevouts, scripts, 0, 1)
  let signature = sign(sighash, privateKey)

  const signatureBuf = Buffer.from(signature)
  tx.setWitness(0, [
    signatureBuf,
    userTakesScript,
    p2tr.witness[p2tr.witness.length - 1],
  ])

  return tx.toHex()
}

const program = new Command()

program
  .name('deposit-reclaim')
  .description('Generate a tx hex for reclaiming a deposit')
  .requiredOption('-p, --private-key <privateKey>', 'Private key')
  .requiredOption('-a, --address <address>', 'Bitcoin address')
  .requiredOption('-e, --evm-address <evmAddress>', 'EVM address')
  .requiredOption('-t, --txid <txid>', 'Txid of the deposit transaction')
  .requiredOption('-v, --vout <vout>', 'Vout of the deposit transaction')
  .requiredOption('-m, --amount <amount>', 'Amount of the deposit transaction')
  .requiredOption('-f, --fee <fee>', 'Fee to pay for the reclaim tx', '0.00001')
  .action(async options => {
    const privateKey = options.privateKey
    const address = options.address
    const evmAddress = options.evmAddress.startsWith('0x')
      ? options.evmAddress.slice(2)
      : options.evmAddress
    const txid = options.txid
    const vout = parseInt(options.vout, 10)
    const amount = parseFloat(options.amount)
    const amountInSats = Math.round(amount * 1e8)
    const fee = parseFloat(options.fee)
    const feeInSats = Math.round(fee * 1e8)

    const txHex = prepareUserTakesBackTx(
      privateKey,
      address,
      evmAddress,
      txid,
      vout,
      amountInSats,
      feeInSats
    )
    console.log('Hex for the reclaim tx:')
    console.log(txHex)
    console.log('You can broadcast the tx using the following link:')
    console.log('https://mempool.space/testnet4/tx/push')
  })

program.parse(process.argv)
