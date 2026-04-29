# Frontend — SorobanLoyalty

Next.js 14 (App Router) frontend for the SorobanLoyalty platform. Users claim and redeem LYT rewards; merchants create campaigns — all signed on-chain via the Freighter browser extension.

---

## Local Development Setup

### Prerequisites

- Node.js 20+
- [Freighter browser extension](https://www.freighter.app/) installed
- Backend API running (see root README)

### Install & run

```bash
cd frontend
npm install
npm run dev
```

App is available at `http://localhost:3000`.

---

## Environment Variables

Copy `.env.example` from the project root and set the following `NEXT_PUBLIC_*` variables:

| Variable                           | Description                        | Default                             |
| ---------------------------------- | ---------------------------------- | ----------------------------------- |
| `NEXT_PUBLIC_API_URL`              | Backend REST API base URL          | `http://localhost:3001`             |
| `NEXT_PUBLIC_SOROBAN_RPC_URL`      | Soroban RPC endpoint               | `http://localhost:8000/soroban/rpc` |
| `NEXT_PUBLIC_NETWORK_PASSPHRASE`   | Stellar network passphrase         | `Test SDF Network ; September 2015` |
| `NEXT_PUBLIC_REWARDS_CONTRACT_ID`  | Deployed rewards contract address  | _(set after deploy)_                |
| `NEXT_PUBLIC_CAMPAIGN_CONTRACT_ID` | Deployed campaign contract address | _(set after deploy)_                |

> All variables are prefixed `NEXT_PUBLIC_` so they are inlined at build time and available in the browser.

---

## Component Architecture

```
src/
├── app/                        # Next.js App Router pages
│   ├── layout.tsx              # Root layout — WalletProvider, I18nProvider, nav
│   ├── page.tsx                # Home / redirect
│   ├── dashboard/page.tsx      # Claim rewards, view balance, infinite-scroll campaigns
│   ├── merchant/page.tsx       # Create & manage campaigns
│   ├── analytics/page.tsx      # Campaign performance stats
│   └── transactions/           # Transaction history
├── components/
│   ├── WalletConnector.tsx     # Connect / disconnect Freighter button
│   ├── CampaignCard.tsx        # Campaign display + claim action
│   ├── RewardList.tsx          # User reward list + redeem action
│   ├── NetworkBanner.tsx       # Degraded / unreachable network warning
│   └── NetworkStatusIndicator.tsx  # Status dot in nav
├── context/
│   ├── WalletContext.tsx       # Freighter public key state, connect/disconnect
│   └── I18nContext.tsx         # i18n locale state (en / es)
├── hooks/
│   └── useNetworkStatus.ts     # Polls /health, exposes { health }
└── lib/
    ├── freighter.ts            # Freighter API wrappers
    ├── soroban.ts              # Contract invocation helpers
    ├── api.ts                  # Backend REST client
    └── export.ts               # CSV / JSON data export helpers
```

### Key data flows

- **Claim reward**: `DashboardPage` → `claimReward()` (lib/soroban.ts) → Freighter signs → Soroban RPC submits → backend indexes event → UI refreshes via `api.getUserRewards()`.
- **Create campaign**: `MerchantPage` → `createCampaign()` (lib/soroban.ts) → Freighter signs → Soroban RPC submits.
- **Read data**: All read operations go through the backend REST API (`lib/api.ts`), not directly to the RPC.

---

## Freighter Integration

Freighter is a Stellar browser extension wallet. The integration lives in:

- `src/lib/freighter.ts` — low-level Freighter wrapper
- `src/context/WalletContext.tsx` — connection state, refresh logic, and toast feedback
- `src/components/WalletConnector.tsx` — install detection and connect UI
- `src/lib/soroban.ts` — building, simulating, signing, and submitting Soroban transactions

### Freighter detection and installation check

The extension injects the wallet API into the browser. The app supports both common Freighter injection patterns:

- `window.freighter`
- `window.freighterApi`

`WalletConnector` checks for either object before attempting to connect and shows an install prompt when neither is present.

Example detection:

```ts
const hasFreighter =
  typeof window !== "undefined" &&
  // @ts-expect-error injected by the extension
  (typeof window.freighter !== "undefined" ||
    typeof window.freighterApi !== "undefined");

if (!hasFreighter) {
  // render install prompt modal
}
```

`src/lib/freighter.ts` also performs a defensive runtime check with `freighter.isConnected()`:

```ts
const connected = await freighter.isConnected();
if (!connected) throw new Error("Freighter extension not installed");
```

### Wallet connection flow

1. `WalletProvider` mounts and calls `getPublicKey()` so any existing Freighter session is restored silently.
2. The user clicks **Connect Freighter** in `WalletConnector`.
3. `WalletConnector` confirms the extension is installed and calls `connectWallet()`.
4. `connectWallet()` calls `freighter.isConnected()` and then `freighter.getPublicKey()`.
5. The returned Stellar public key is stored in context and used for all future contract calls.
6. `disconnect()` clears the public key from local state; it does not revoke the Freighter authorization session.

Example usage:

```ts
import { connectWallet } from "@/lib/freighter";

async function onConnect() {
  const publicKey = await connectWallet();
  console.log("Connected wallet:", publicKey);
}
```

### Transaction signing flow

The transaction flow is:

1. Build a Soroban transaction in `src/lib/soroban.ts`.
2. Simulate the transaction using the configured RPC.
3. Assemble the prepared transaction from the simulation result.
4. Request Freighter to sign the XDR with the configured network passphrase.
5. Submit the signed transaction back to the RPC.

This is handled by `signTransaction()` in `src/lib/freighter.ts`:

```ts
import { signTransaction } from "@/lib/freighter";

const signedXdr = await signTransaction(unsignedTxXdr, networkPassphrase);
```

The core transaction helper is `invokeContract()` in `src/lib/soroban.ts`, which simulates, signs, sends, and polls until confirmation.

### Error handling and failure modes

The app handles common Freighter failure modes at multiple levels:

- Missing extension: `freighter.isConnected()` returns `false`, and the UI prompts installation.
- Connection denied: `getPublicKey()` returns an error and the app keeps the wallet disconnected.
- Sign request rejected: `signTransaction()` returns an error; the UI reports that the user cancelled or rejected the transaction.
- Invalid transaction simulation: `simulateTransaction()` throws before signing, so invalid contract inputs are caught early.
- Transaction submit failure: `sendTransaction()` may return an error status, which is surfaced to the caller.
- Network mismatch: Freighter must use the same network as `NEXT_PUBLIC_NETWORK_PASSPHRASE`.

For example, `connectWallet()` and `signTransaction()` both throw if Freighter returns an `error` field:

```ts
const { publicKey, error } = await freighter.getPublicKey();
if (error) throw new Error(error);
```

### Network switching (Testnet / Mainnet)

The frontend uses `NEXT_PUBLIC_NETWORK_PASSPHRASE` to determine the active Stellar network.

- Testnet: `Test SDF Network ; September 2015`
- Mainnet: `Public Global Stellar Network ; September 2015`

Freighter must be configured to the same network in its extension settings. If the network differs, transaction signing may fail or the signed transaction may be rejected.

Example environment variable:

```env
NEXT_PUBLIC_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
```

### Freighter compatibility

This project is designed to work with both Freighter v1.x and v2.x.

- Both versions expose the same core wallet APIs through `@stellar/freighter-api`.
- The app detects either the `window.freighter` or `window.freighterApi` injection.
- `src/lib/freighter.ts` is intentionally minimal and only relies on `isConnected()`, `getPublicKey()`, and `signTransaction()`.

### Example flow: claim a reward

```ts
import { claimReward } from "@/lib/soroban";
await claimReward(publicKey, campaignId);
```

1. `claimReward()` builds the transaction and simulates it.
2. `signTransaction()` prompts Freighter to sign the prepared XDR.
3. The signed transaction is submitted to the Soroban RPC.
4. The backend indexes the resulting on-chain event and the UI refreshes.

### Testing

- Unit tests cover `src/lib/freighter.ts` helpers and `WalletConnector` detection.
- `e2e/tests/wallet-connection.spec.ts` covers the wallet connection flow end-to-end.
- Compatibility tests verify detection for both `window.freighter` and `window.freighterApi`.

---

## Testing

### Unit / integration tests

The project uses the Next.js built-in Jest setup. Run:

```bash
npm test
```

### E2E tests (Playwright)

E2E tests live in `../e2e/` and cover wallet connection, campaign listing, and campaign detail flows.

```bash
# From the e2e directory
cd ../e2e
npm install
npx playwright install
npx playwright test
```

Tests run against a live local stack (frontend + backend + local Soroban node). Start the full stack first:

```bash
# From project root
docker-compose up --build
```

---

## Deployment

### Vercel

1. Import the repository into Vercel.
2. Set **Root Directory** to `frontend`.
3. Add all `NEXT_PUBLIC_*` environment variables in the Vercel dashboard.
4. Deploy — Vercel detects Next.js automatically.

### Docker

A production image is included:

```bash
# Build
docker build -t soroban-loyalty-frontend ./frontend

# Run
docker run -p 3000:3000 \
  -e NEXT_PUBLIC_API_URL=https://api.example.com \
  -e NEXT_PUBLIC_SOROBAN_RPC_URL=https://soroban-testnet.stellar.org \
  -e NEXT_PUBLIC_NETWORK_PASSPHRASE="Test SDF Network ; September 2015" \
  -e NEXT_PUBLIC_REWARDS_CONTRACT_ID=<contract_id> \
  -e NEXT_PUBLIC_CAMPAIGN_CONTRACT_ID=<contract_id> \
  soroban-loyalty-frontend
```

Or use the root `docker-compose.yml` which wires all services together:

```bash
docker-compose up --build
```

### Build output

The Next.js config uses `output: "standalone"` — the build artefact in `.next/standalone` is self-contained and can be deployed to any Node.js host.
