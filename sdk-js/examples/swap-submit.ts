import { StellarRouteClient } from '../src/index.js';

// IMPORTANT: Never hardcode production secrets in your application.
// For demonstration, use a testnet account public key here.
const SENDER_PUBLIC_KEY = 'GABC...'; // Replace with a real testnet public key
const USDC_TESTNET = 'USDC:GBBD47IF6LWK7P7MDEVSCWTTCJMPN2S4RY3G5GNCY7G1MNC2S4RY3G5G';

async function main() {
  // Use a testnet API endpoint
  const client = new StellarRouteClient({ baseUrl: 'https://api.testnet.stellarroute.io' });

  console.log('1. Fetching ranked routes for 100 XLM -> USDC...');
  const routesResponse = await client.getRankedRoutes('native', USDC_TESTNET, 100);
  
  if (routesResponse.routes.length === 0) {
    console.log('No routes found.');
    return;
  }

  const bestRoute = routesResponse.routes[0];
  console.log(`Best route estimated output: ${bestRoute.estimated_output} USDC`);

  console.log('\n2. Preparing and submitting swap transaction...');
  try {
    const result = await client.executeSwap({
      route: { hops: bestRoute.path },
      amount: '100',
      sender: SENDER_PUBLIC_KEY,
      slippage_bps: 50,
      signTransaction: async (xdrBase64) => {
        console.log(' >> Received unsigned XDR from backend.');
        // In a real application, you would sign the transaction using the Stellar SDK:
        // import * as StellarSdk from '@stellar/stellar-sdk';
        // const tx = new StellarSdk.Transaction(xdrBase64, StellarSdk.Networks.TESTNET);
        // tx.sign(StellarSdk.Keypair.fromSecret(SENDER_SECRET_KEY));
        // return tx.toXDR();
        
        // For this example, we just return a dummy signed XDR.
        console.log(' >> Signing transaction...');
        return xdrBase64; 
      }
    });

    console.log(`\nSwap submitted successfully!`);
    console.log(`Transaction Hash: ${result.tx_hash}`);
    console.log(`Status: ${result.status}`);
  } catch (error) {
    console.error('\nSwap failed:', error);
  }
}

main().catch(console.error);
