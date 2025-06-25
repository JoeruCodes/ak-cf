# Real Crypto Exchange System

This system allows players to exchange in-game currency (akai) for real cryptocurrencies using Alloy.rs for Ethereum blockchain interactions.

## ✅ **Real Blockchain Transactions**

- **Native Token Transfers** (ETH, MATIC): Direct transfers using Alloy.rs
- **ERC-20 Token Transfers** (USDC): Smart contract interactions  
- **Real Transaction Signing**: Uses your private key to sign transactions
- **Actual Blockchain Interaction**: Sends transactions to Ethereum network
- **Transaction Hash Tracking**: Returns real transaction hashes
- **Transaction Receipts**: Waits for transaction confirmation

## Setup

### 1. Environment Variables

Add these to your Cloudflare Workers environment:

```bash
# Ethereum RPC URL (use your preferred provider)
ETHEREUM_RPC_URL=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
# or for testnet: https://sepolia.infura.io/v3/YOUR_PROJECT_ID

# Your wallet private key (keep this secure!)
WALLET_PRIVATE_KEY=0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef
```

### 2. Wallet Setup

1. Create a new Ethereum wallet or use an existing one
2. Fund it with the cryptocurrencies you want to distribute
3. Add the private key to your environment variables
4. **IMPORTANT**: Use a dedicated wallet for this purpose, not your main wallet

### 3. Supported Cryptocurrencies

Currently supported:
- **ETH** (Ethereum): 1000 akai = 1 ETH (requires 100 IQ)
- **USDC** (USD Coin): 1 akai = 1 USDC (requires 50 IQ)
- **MATIC** (Polygon): 500 akai = 1 MATIC (requires 75 IQ)

## Usage

### Frontend Integration

```javascript
// Get available cryptocurrencies
const response = await fetch('/api/game', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
        op: 'GetAvailableCryptos'
    })
});

const data = await response.json();
console.log('Available cryptos:', data.available_cryptos);

// Exchange akai for crypto
const exchangeResponse = await fetch('/api/game', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
        op: 'ExchangeAkaiForCrypto',
        akai_amount: 1000,
        crypto_symbol: 'ETH',
        user_wallet_address: '0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6'
    })
});

const exchangeData = await exchangeResponse.json();
if (exchangeData.status === 'Exchange successful') {
    console.log('Transaction hash:', exchangeData.transaction_hash);
    console.log('Crypto amount sent:', exchangeData.crypto_amount);
    console.log('Block confirmation:', exchangeData.block_number);
}
```

### API Endpoints

#### Get Available Cryptocurrencies
```json
{
    "op": "GetAvailableCryptos"
}
```

Response:
```json
{
    "available_cryptos": [
        {
            "symbol": "ETH",
            "name": "Ethereum",
            "contract_address": null,
            "min_iq_required": 100,
            "exchange_rate": 1000.0,
            "decimals": 18,
            "is_active": true
        }
    ],
    "user_iq": 150,
    "akai_balance": 5000
}
```

#### Exchange Akai for Crypto
```json
{
    "op": "ExchangeAkaiForCrypto",
    "akai_amount": 1000,
    "crypto_symbol": "ETH",
    "user_wallet_address": "0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6"
}
```

Success Response:
```json
{
    "status": "Exchange successful",
    "transaction_hash": "0x1234567890abcdef...",
    "crypto_amount": 1.0,
    "akai_deducted": 1000,
    "new_akai_balance": 4000,
    "crypto_symbol": "ETH",
    "wallet_address": "0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6"
}
```

## 🔥 **Real Transaction Features**

### Native Token Transfers (ETH, MATIC)
- Uses `Unit::ETHER.wei()` for proper amount conversion
- Automatically handles gas estimation
- Waits for transaction confirmation
- Returns real transaction hash

### ERC-20 Token Transfers (USDC)
- Encodes `transfer(address,uint256)` function calls
- Handles token decimals correctly
- Supports any ERC-20 token
- Returns transaction receipt with block number

### Transaction Monitoring
```
Pending transaction... 0x1234567890abcdef...
Transaction included in block 12345678
Transferred 1.00000 ETH to 0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6
```

## Security Considerations

1. **Private Key Security**: Never expose your private key in client-side code
2. **Rate Limiting**: Implement rate limiting to prevent abuse
3. **Balance Monitoring**: Monitor your wallet balance regularly
4. **Gas Fees**: Consider gas fees when setting exchange rates
5. **Network Selection**: Use appropriate networks (mainnet vs testnet)
6. **Transaction Validation**: All transactions are validated before sending

## Testing

For testing, use Ethereum testnets:
- **Sepolia**: `https://sepolia.infura.io/v3/YOUR_PROJECT_ID`
- **Goerli**: `https://goerli.infura.io/v3/YOUR_PROJECT_ID`

## Monitoring

The system logs all transactions:
```
Real crypto exchange completed: 1.0 ETH sent to 0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6 (tx: 0x1234567890abcdef...)
```

## Customization

You can modify the crypto options in `crypto.rs`:
- Add new cryptocurrencies
- Change exchange rates
- Adjust IQ requirements
- Update contract addresses

## Error Handling

Common errors:
- `Insufficient akai balance`: User doesn't have enough akai
- `Insufficient IQ`: User's IQ is below the requirement
- `Crypto not found`: Invalid cryptocurrency symbol
- `Invalid wallet address`: Malformed Ethereum address
- `Transaction failed`: Blockchain transaction failed (check gas fees, network issues)
- `Failed to get transaction receipt`: Network issues or transaction reverted

## 🚀 **Ready for Production**

The system is now ready for **real cryptocurrency transactions**! Players can exchange their in-game akai for actual ETH, USDC, or MATIC tokens that will be sent to their Ethereum wallets with full blockchain confirmation. 