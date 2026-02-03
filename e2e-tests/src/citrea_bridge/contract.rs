use alloy::sol;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    BridgeContract,
    "./resources/Bridge.json"
);
