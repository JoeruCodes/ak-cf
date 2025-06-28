use crate::types::{CryptoInfo};
use alloy::{
    network::TransactionBuilder,
    primitives::utils::parse_units,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use serde::Deserialize;
use std::collections::HashMap;
use worker::{Fetch, Request, RequestInit, Response, Method};

const AKAI_RATE_IN_USDT: f64 = 0.01;

#[derive(Deserialize, Debug)]
struct CoinGeckoPrice {
    usd: f64,
}

/// Static list of supported cryptos
pub fn all_cryptos() -> Vec<CryptoInfo> {
    vec![
        // Meme Coins
        CryptoInfo {
            symbol: "SHIB".to_string(),
            name: "Shiba Inu".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 200,
            api_id: "shiba-inu".to_string(),
            contract_address: Some("0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "PEPE".to_string(),
            name: "Pepe".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 200,
            api_id: "pepe".to_string(),
            contract_address: Some("0x6982508145454ce325ddbe47a25d4ec3d2311933".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "TURBO".to_string(),
            name: "Turbo".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 200,
            api_id: "turbo".to_string(),
            contract_address: Some("0xa35923162c49cf95e6bf26623385eb431ad920d3".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "CORGIAI".to_string(),
            name: "CorgiAI".to_string(),
            network: "cronos".to_string(),
            rpc_url: "https://evm.cronos.org/".to_string(),
            min_iq: 300,
            api_id: "corgiai".to_string(),
            contract_address: Some("0x6b431b8a964bfcf28191b07c91189ff4403957d0".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "FLOKI".to_string(),
            name: "Floki".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 300,
            api_id: "floki".to_string(),
            contract_address: Some("0xcf0c122c6b73ff809c693db761e7baebe62b6a2e".to_string()),
            decimals: 9,
        },

        // Native Coins
        CryptoInfo {
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 400,
            api_id: "ethereum".to_string(),
            contract_address: None,
            decimals: 18,
        },
        CryptoInfo {
            symbol: "BNB".to_string(),
            name: "BNB".to_string(),
            network: "bsc".to_string(),
            rpc_url: "https://bsc-dataseed.binance.org/".to_string(),
            min_iq: 400,
            api_id: "binancecoin".to_string(),
            contract_address: None,
            decimals: 18,
        },
        CryptoInfo {
            symbol: "SKL".to_string(),
            name: "Skale".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 500,
            api_id: "skale".to_string(),
            contract_address: Some("0x00c83aecc790e8a4453e5dd3b0b4b3680501a7a7".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "POL".to_string(),
            name: "Polygon".to_string(),
            network: "ethereum".to_string(),
            rpc_url: "https://cloudflare-eth.com".to_string(),
            min_iq: 500,
            api_id: "polygon".to_string(),
            contract_address: Some("0x455e53CBB86018Ac2B8092FdCd39d8444aFFC3F6".to_string()),
            decimals: 18,
        },
        CryptoInfo {
            symbol: "WAVAX".to_string(),
            name: "Wrapped AVAX".to_string(),
            network: "avalanche".to_string(),
            rpc_url: "https://api.avax.network/ext/bc/C/rpc".to_string(),
            min_iq: 500,
            api_id: "avalanche-2".to_string(),
            contract_address: Some("0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7".to_string()),
            decimals: 18,
        },
    ]
}

sol! {
    // Other contract items can be placed here
    #[sol(rpc)] // This will generate a Rust type that can be used to interact with the contract.
    contract ERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

/// Fetch live price from CoinGecko
async fn fetch_live_usdt_price(api_id: &str) -> Result<f64, worker::Error> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
        api_id
    );
    let req = Request::new(&url, Method::Get)?;
    let mut res = Fetch::Request(req).send().await?;
    
    if !res.status_code() == 200 {
        return Err(worker::Error::from(format!("CoinGecko API returned status {}", res.status_code())));
    }

    let price_data: HashMap<String, CoinGeckoPrice> = res.json().await?;
    
    price_data.get(api_id)
        .map(|price| price.usd)
        .ok_or_else(|| worker::Error::from(format!("Price not found for {}", api_id)))
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

    worker::console_log!("Attempting to connect to RPC provider...");
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&rpc_url)
        .await
        .map_err(|e| e.to_string())?;
    worker::console_log!("Successfully connected to RPC provider.");

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

    worker::console_log!("Attempting to connect to RPC provider (for ERC20)...");
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&rpc_url)
        .await
        .map_err(|e| e.to_string())?;
    worker::console_log!("Successfully connected to RPC provider (for ERC20).");

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

/// Calculate the amount of crypto a user gets for a certain amount of Akai
pub async fn calculate_crypto_amount(
    akai_amount: usize,
    user_iq: usize,
    crypto_symbol: &str,
) -> Result<f64, String> {
    let cryptos = all_cryptos();
    let crypto = cryptos
        .iter()
        .find(|c| c.symbol == crypto_symbol)
        .ok_or_else(|| "Crypto not found".to_string())?;

    if user_iq < crypto.min_iq {
        return Err("User IQ is too low for this crypto".to_string());
    }

    let live_price = fetch_live_usdt_price(&crypto.api_id).await.map_err(|e| e.to_string())?;

    let akai_value_in_usdt = AKAI_RATE_IN_USDT * (user_iq as f64 / 100.0);
    let total_usdt_value = akai_amount as f64 * akai_value_in_usdt;
    let crypto_amount = total_usdt_value / live_price;

    Ok(crypto_amount)
}

/// Exchange Akai for crypto using real blockchain logic
pub async fn exchange_akai_for_crypto_real(
    akai_amount: usize,
    crypto_symbol: &str,
    user_wallet_address: &str,
    user_iq: usize,
    user_akai_balance: usize,
    private_key: &str,
) -> Result<String, String> {
    worker::console_log!("Entering exchange_akai_for_crypto_real...");
    let cryptos = all_cryptos();
    let crypto = cryptos
        .iter()
        .find(|c| c.symbol == crypto_symbol && user_wallet_address.starts_with("0x"))
        .ok_or_else(|| "Crypto not available or invalid address".to_string())?;

    if user_iq < crypto.min_iq {
        return Err("User IQ is too low for this crypto".to_string());
    }
    if akai_amount > user_akai_balance {
        return Err("Insufficient akai balance".to_string());
    }

    let amount = calculate_crypto_amount(akai_amount, user_iq, crypto_symbol).await?;

    if crypto.contract_address.is_none()
    {
        send_eth(
            &crypto.rpc_url,
            private_key,
            user_wallet_address,
            amount,
        )
        .await
    } else {
        send_erc20(
            &crypto.rpc_url,
            private_key,
            crypto.contract_address.as_ref().unwrap(),
            user_wallet_address,
            amount,
            crypto.decimals,
        )
        .await
    }
}
