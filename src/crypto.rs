use crate::types::{CryptoInfo, ExchangeRequest};
use alloy::{
    network::TransactionBuilder,
    primitives::utils::parse_units,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};

/// Static list of supported cryptos
pub fn all_cryptos() -> Vec<CryptoInfo> {
    vec![
        CryptoInfo {
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://mainnet.infura.io/v3/your_key".to_string(),
            min_iq: 10,
            exchange_rate: 0.0005, // 1 akai = 0.0005 ETH
            contract_address: None,
            decimals: 18,
        },
        CryptoInfo {
            symbol: "USDT".to_string(),
            name: "Tether USD".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://mainnet.infura.io/v3/your_key".to_string(),
            min_iq: 20,
            exchange_rate: 0.8, // 1 akai = 0.8 USDT
            contract_address: Some("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()),
            decimals: 6,
        },
        CryptoInfo {
            symbol: "MATIC".to_string(),
            name: "Polygon".to_string(),
            network: "polygon".to_string(),
            rpc_url: "https://polygon-rpc.com".to_string(),
            min_iq: 30,
            exchange_rate: 1.2, // 1 akai = 1.2 MATIC
            contract_address: None,
            decimals: 18,
        },
        CryptoInfo {
            symbol: "USDT".to_string(),
            name: "Tether USD (Polygon)".to_string(),
            network: "polygon".to_string(),
            rpc_url: "https://polygon-rpc.com".to_string(),
            min_iq: 30,
            exchange_rate: 0.8, // 1 akai = 0.8 USDT
            contract_address: Some("0x3813e82e6f7098b9583FC0F33a962D02018B6803".to_string()),
            decimals: 6,
        },
        CryptoInfo {
            symbol: "BNB".to_string(),
            name: "BNB".to_string(),
            network: "bsc".to_string(),
            rpc_url: "https://bsc-dataseed.binance.org/".to_string(),
            min_iq: 30,
            exchange_rate: 0.5, // 1 akai = 0.5 BNB
            contract_address: None,
            decimals: 18,
        },
        CryptoInfo {
            symbol: "USDT".to_string(),
            name: "Tether USD (BSC)".to_string(),
            network: "bsc".to_string(),
            rpc_url: "https://bsc-dataseed.binance.org/".to_string(),
            min_iq: 30,
            exchange_rate: 0.8, // 1 akai = 0.8 USDT
            contract_address: Some("0x55d398326f99059fF775485246999027B3197955".to_string()),
            decimals: 18,
        },
    ]
}

/// Get all cryptos (no filtering)
pub fn get_available_cryptos(_user_iq: usize) -> Vec<CryptoInfo> {
    all_cryptos()
}

sol! {
    // Other contract items can be placed here
    #[sol(rpc)] // This will generate a Rust type that can be used to interact with the contract.
    contract ERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

/// Send ETH using alloy
pub async fn send_eth(
    rpc_url: &str,
    private_key: &str,
    to: &str,
    amount_eth: f64,
) -> Result<String, String> {
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|e| format!("Invalid private key: {e}"))?;
    let to_addr = to
        .parse()
        .map_err(|e| format!("Invalid recipient address: {e}"))?;

    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&rpc_url).await.map_err(|e|e.to_string())?;

    let amount_str = amount_eth.to_string();
    let value = parse_units(&amount_str, "ether").unwrap().into();

    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_to(to_addr)
        .with_value(value);

    let pending_tx = provider
        .send_transaction(tx)
        .await
        .map_err(|e| format!("Send error: {e}"))?;
    Ok(format!("{}", pending_tx.tx_hash()))
}

/// Send ERC20 using alloy
pub async fn send_erc20(
    rpc_url: &str,
    private_key: &str,
    contract_address: &str,
    to: &str,
    amount: f64,
    decimals: u8,
) -> Result<String, String> {
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|e| format!("Invalid private key: {e}"))?;

    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&rpc_url).await.map_err(|e|e.to_string())?;

    let token_address = contract_address
        .parse()
        .map_err(|e| format!("Invalid contract address: {e}"))?;
    let contract = ERC20::new(token_address, provider);
    let recipient = to
        .parse()
        .map_err(|e| format!("Invalid recipient address: {e}"))?;

    let amount_str = amount.to_string();
    let amount_u256 = parse_units(&amount_str, decimals).unwrap().into();

    let pending_tx = contract
        .transfer(recipient, amount_u256)
        .send()
        .await
        .map_err(|e| format!("Send error: {e}"))?;
    Ok(format!("{}", pending_tx.tx_hash()))
}

/// Exchange Akai for crypto using real blockchain logic
pub async fn exchange_akai_for_crypto_real(
    req: &ExchangeRequest,
    user_iq: usize,
    user_akai_balance: usize,
    private_key: &str,
) -> Result<String, String> {
    let cryptos = get_available_cryptos(user_iq);
    let crypto = cryptos
        .iter()
        .find(|c| c.symbol == req.crypto_symbol && req.user_wallet_address.starts_with("0x"))
        .ok_or_else(|| "Crypto not available for your IQ or invalid address".to_string())?;
    if req.akai_amount > user_akai_balance {
        return Err("Insufficient akai balance".to_string());
    }
    let amount = (req.akai_amount as f64) * crypto.exchange_rate;
    if amount <= 0.0 {
        return Err("Amount too low".to_string());
    }
    if crypto.symbol == "ETH"
        || crypto.symbol == "MATIC"
        || crypto.symbol == "BNB"
        || crypto.contract_address.is_none()
    {
        send_eth(
            &crypto.rpc_url,
            private_key,
            &req.user_wallet_address,
            amount,
        )
        .await
    } else {
        send_erc20(
            &crypto.rpc_url,
            private_key,
            crypto.contract_address.as_ref().unwrap(),
            &req.user_wallet_address,
            amount,
            crypto.decimals,
        )
        .await
    }
}
