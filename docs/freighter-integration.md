# Freighter Wallet Integration Guide

## Overview

This document provides comprehensive guidance for frontend developers implementing or extending Freighter wallet integration in SorobanLoyalty. It covers detection, connection, transaction signing, error handling, and network switching.

---

## Freighter Detection and Installation

### Detecting Freighter Installation

Freighter injects wallet API into the browser at either `window.freighter` or `window.freighterApi` depending on the version:

```typescript
// Check if Freighter is installed
const hasFreighter =
  typeof window !== "undefined" &&
  (typeof window.freighter !== "undefined" ||
    typeof window.freighterApi !== "undefined");

if (!hasFreighter) {
  // Prompt user to install Freighter
}
```

The app supports both injection patterns to maintain compatibility with Freighter v1.x (uses `window.freighter`) and v2.x (may use `window.freighterApi`).

### Runtime Installation Check

The `freighter.isConnected()` API provides a runtime check that confirms the extension is available and can be invoked:

```typescript
import freighter from "@stellar/freighter-api";

try {
  const connected = await freighter.isConnected();
  if (!connected) {
    throw new Error("Freighter extension not installed");
  }
} catch (error) {
  console.error("Freighter check failed:", error);
}
```

### Installation Prompt UI

When Freighter is not detected, `WalletConnector` displays a modal with install links:

```typescript
// Available in: src/components/WalletConnector.tsx
function FreighterModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="modal">
      <h2>Freighter Wallet Required</h2>
      <p>Install Freighter to sign transactions securely.</p>
      <a href="https://chrome.google.com/webstore/detail/freighter/bcacfldlkkdogcmkkibnjlakofdplcbk">
        Install for Chrome
      </a>
      <a href="https://addons.mozilla.org/en-US/firefox/addon/freighter/">
        Install for Firefox
      </a>
    </div>
  );
}
```

---

## Wallet Connection Flow

### Step-by-step Connection Process

1. **Initial Mount**: `WalletProvider` calls `getPublicKey()` to restore any existing session.
2. **User Initiates Connect**: User clicks "Connect Freighter" button in `WalletConnector`.
3. **Extension Detection**: `WalletConnector` confirms Freighter is installed.
4. **Request Authorization**: Calls `connectWallet()`, which invokes `freighter.getPublicKey()`.
5. **Freighter Popup**: User approves or denies the connection in the Freighter extension popup.
6. **State Update**: On approval, the public key is stored in `WalletContext` state.
7. **Subsequent Calls**: All contract invocations use the stored public key.

### Implementation Example

```typescript
import { useWallet } from "@/context/WalletContext";

export function ConnectExample() {
  const { publicKey, connecting, connect, disconnect } = useWallet();

  if (publicKey) {
    return (
      <div>
        <span>Connected: {publicKey.slice(0, 6)}…{publicKey.slice(-4)}</span>
        <button onClick={disconnect}>Disconnect</button>
      </div>
    );
  }

  return (
    <button onClick={connect} disabled={connecting}>
      {connecting ? "Connecting…" : "Connect Freighter"}
    </button>
  );
}
```

### Session Persistence

Freighter maintains its own session. When the user returns to the app:

- `WalletProvider` calls `getPublicKey()` on mount.
- If Freighter still has an active session, the key is restored **without** prompting the user again.
- If the session expired, `getPublicKey()` returns `null` and the user must reconnect.

### Disconnect Behavior

Calling `disconnect()` **does not revoke** the Freighter authorization. It only clears the public key from the app's local state. The Freighter session remains active, so the next connection attempt will restore the wallet without prompting again.

---

## Transaction Signing Flow

### Overview

Signing a transaction involves:

1. Building the transaction in the contract helper (e.g., `claimReward()`).
2. Simulating the transaction to compute resource fees and validate inputs.
3. Preparing the transaction with the simulation result.
4. Requesting Freighter to sign the prepared transaction XDR.
5. Submitting the signed transaction to the Soroban RPC.
6. Polling for confirmation.

### Detailed Flow Diagram

```
claimReward(publicKey, campaignId)
  ↓
invokeContract(publicKey, REWARDS_CONTRACT_ID, "claim_reward", [args])
  ↓
Build TransactionBuilder with Freighter's current account info
  ↓
simulateTransaction() — validates inputs, computes fees
  ↓
assembleTransaction() — prepares final transaction with simulation result
  ↓
signTransaction(prepared.toXDR(), networkPassphrase)
  ↓
Freighter extension displays transaction details to user
  ↓
User approves or rejects in Freighter
  ↓
Receive signed XDR
  ↓
sendTransaction(signedXdrTransaction) — submit to Soroban RPC
  ↓
Poll getTransaction() until confirmed or failed
```

### Code Example: Claim Reward

```typescript
// Using the high-level API
import { claimReward } from "@/lib/soroban";

const tx = await claimReward(publicKey, campaignId);
// tx.resultMetaXdr contains the success result
```

### Code Example: Building a Custom Contract Call

If you need to invoke a different contract method:

```typescript
import {
  SorobanRpc,
  TransactionBuilder,
  Contract,
  nativeToScVal,
  Address,
} from "@stellar/stellar-sdk";
import { signTransaction } from "@/lib/freighter";

const NETWORK_PASSPHRASE = process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE!;
const RPC_URL = process.env.NEXT_PUBLIC_SOROBAN_RPC_URL!;

const server = new SorobanRpc.Server(RPC_URL, { allowHttp: true });

async function customContractCall(
  publicKey: string,
  contractId: string,
  method: string,
  args: ScVal[],
) {
  // Step 1: Get account info
  const account = await server.getAccount(publicKey);

  // Step 2: Build transaction
  const contract = new Contract(contractId);
  const tx = new TransactionBuilder(account, {
    fee: "100", // stroops per operation
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  // Step 3: Simulate
  const simResult = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Simulation failed: ${simResult.error}`);
  }

  // Step 4: Assemble
  const preparedTx = SorobanRpc.assembleTransaction(tx, simResult).build();

  // Step 5: Sign (user sees Freighter popup here)
  const signedXdr = await signTransaction(
    preparedTx.toXDR(),
    NETWORK_PASSPHRASE,
  );

  // Step 6: Submit
  const submitResult = await server.sendTransaction(
    TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE),
  );
  if (submitResult.status === "ERROR") {
    throw new Error(
      `Submit failed: ${submitResult.errorResult?.toXDR("base64")}`,
    );
  }

  // Step 7: Poll for confirmation
  let getResult = await server.getTransaction(submitResult.hash);
  let attempts = 0;
  while (
    getResult.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND &&
    attempts < 20
  ) {
    await new Promise((r) => setTimeout(r, 1500));
    getResult = await server.getTransaction(submitResult.hash);
    attempts++;
  }

  if (getResult.status !== SorobanRpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`Transaction failed: ${getResult.status}`);
  }

  return getResult;
}
```

---

## Error Handling

### Common Failure Modes

#### 1. Extension Not Installed

- **When**: `freighter.isConnected()` returns `false` or `typeof window.freighter === "undefined"`
- **User Action**: Show install prompt modal
- **Code**:
  ```typescript
  const hasFreighter = await freighter.isConnected();
  if (!hasFreighter) {
    setShowInstallModal(true);
  }
  ```

#### 2. Connection Denied / Revoked

- **When**: `getPublicKey()` returns an error field or empty publicKey
- **User Action**: Inform user that wallet authorization was denied
- **Code**:
  ```typescript
  const { publicKey, error } = await freighter.getPublicKey();
  if (error || !publicKey) {
    throw new Error("Connection denied or revoked");
  }
  ```

#### 3. Transaction Rejected by User

- **When**: User clicks "Reject" in the Freighter signing popup
- **Freighter Returns**: `{ signedTxXdr: "", error: "user rejected" }`
- **User Action**: Explain that the user cancelled the transaction
- **Code**:
  ```typescript
  const { signedTxXdr, error } = await freighter.signTransaction(xdr, opts);
  if (error) {
    if (error.toLowerCase().includes("reject")) {
      throw new Error("Transaction rejected by user");
    }
    throw new Error(error);
  }
  ```

#### 4. Invalid Transaction (Simulation Failure)

- **When**: Contract inputs are invalid or account has insufficient balance
- **RPC Returns**: `{ status: "ERROR", error: "..." }`
- **User Action**: Display validation error; user does not reach Freighter popup
- **Code**:
  ```typescript
  const simResult = await server.simulateTransaction(tx);
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Invalid transaction: ${simResult.error}`);
  }
  ```

#### 5. Network Mismatch

- **When**: Freighter is on Testnet but app is configured for Mainnet (or vice versa)
- **Symptom**: Transaction signing succeeds but RPC rejects the signed transaction
- **User Action**: Prompt user to switch networks in Freighter settings
- **Code**:
  ```typescript
  // Check in app initialization
  const NETWORK = process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE!;
  if (!NETWORK.includes("Test") && !NETWORK.includes("Public")) {
    console.warn("Unknown network passphrase:", NETWORK);
  }
  ```

#### 6. Transaction Submission Failure

- **When**: RPC rejects the signed transaction (e.g., sequence number mismatch)
- **RPC Returns**: `{ status: "ERROR", errorResult: { ... } }`
- **User Action**: Inform user that transaction failed to submit; they may retry
- **Code**:
  ```typescript
  const submitResult = await server.sendTransaction(txBuilder);
  if (submitResult.status === "ERROR") {
    throw new Error(
      `Transaction submit failed: ${submitResult.errorResult?.toXDR("base64")}`,
    );
  }
  ```

### Error Parsing and User Messaging

The app includes a structured error parser in `src/utils/sorobanErrorParser.ts` that maps internal errors to user-friendly messages:

```typescript
import { SorobanErrorParser } from "@/utils/sorobanErrorParser";

async function claimRewardSafely(publicKey: string, campaignId: number) {
  try {
    await claimReward(publicKey, campaignId);
  } catch (error) {
    const parsed = SorobanErrorParser.parse(error);
    showToast(parsed.userMessage, "error");
    logger.error("Claim failed:", parsed);
  }
}
```

---

## Network Switching (Testnet / Mainnet)

### Configuring the Network

The app uses the environment variable `NEXT_PUBLIC_NETWORK_PASSPHRASE` to determine the active network:

```env
# Testnet
NEXT_PUBLIC_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"

# Mainnet
NEXT_PUBLIC_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
```

This passphrase is baked into the build and passed to all Freighter signing calls.

### Freighter Network Configuration

Users must configure Freighter to the same network. Steps:

1. Open the Freighter extension.
2. Click the **Settings** icon.
3. Select **Network** or similar option.
4. Choose **Testnet** or **Mainnet**.
5. Reload the app.

### Network Mismatch Scenario

If Freighter and the app are on different networks:

- **Best case**: RPC rejects the signed transaction with a network error.
- **Worst case**: Transaction is silently not found on the RPC (user sees "pending" forever).

### Detecting Network Mismatch

You can check the RPC health endpoint to confirm the expected network, but the definitive test is attempting a small transaction and seeing if it's accepted by the RPC.

```typescript
// Validate network on app load
async function validateNetworkMatch() {
  try {
    const server = new SorobanRpc.Server(RPC_URL);
    const ledger = await server.getLedgerEntries([]); // lightweight call
    console.log("RPC network: OK");
  } catch (error) {
    console.warn("RPC unreachable or network mismatch");
  }
}
```

---

## Freighter Version Compatibility

### Supported Versions

- **Freighter v1.x**: Injects `window.freighter`, uses `@stellar/freighter-api` v5.x–v6.x
- **Freighter v2.x**: Injects `window.freighterApi`, uses `@stellar/freighter-api` v6.x

### Detection Strategy

The app checks for both injection patterns:

```typescript
const hasFreighter =
  typeof window.freighter !== "undefined" ||
  typeof window.freighterApi !== "undefined";
```

### API Surface Compatibility

Both versions implement the same core APIs through `@stellar/freighter-api`:

- `isConnected(): Promise<boolean>`
- `getPublicKey(): Promise<{ publicKey: string; error: string | null }>`
- `signTransaction(xdr: string, opts: SignTxOptions): Promise<{ signedTxXdr: string; error: string | null }>`

The wrapper in `src/lib/freighter.ts` is deliberately minimal and only uses these three methods, ensuring compatibility across both versions.

### Testing Compatibility

The test suite includes compatibility checks for both injection patterns:

```typescript
// src/__tests__/WalletConnector.test.tsx
test("detects Freighter using window.freighterApi alias", async () => {
  delete window.freighter; // Simulate v2.x
  window.freighterApi = {}; // v2.x injection

  const ctx = makeCtx();
  wrap(ctx);
  fireEvent.click(screen.getByRole("button"));
  await Promise.resolve();
  expect(ctx.connect).toHaveBeenCalledTimes(1);
});
```

---

## Testing Freighter Integration

### Unit Tests

Unit tests verify the Freighter wrapper functions in isolation:

- **File**: `src/__tests__/freighterHelpers.test.tsx`
- **Coverage**:
  - `connectWallet()` resolves when installed and authorized
  - `connectWallet()` throws when not installed
  - `getPublicKey()` returns null on error
  - `signTransaction()` resolves signed XDR on success
  - `signTransaction()` throws on rejection

Run with:

```bash
npm test -- src/__tests__/freighterHelpers.test.tsx
```

### UI Component Tests

Component tests verify UI behavior around wallet connection:

- **File**: `src/__tests__/WalletConnector.test.tsx`
- **Coverage**:
  - Shows "Connect" button when disconnected
  - Shows "Connecting…" state while connecting
  - Calls `connect()` on button click
  - Detects both `window.freighter` and `window.freighterApi`
  - Shows disconnect button and balance when connected

Run with:

```bash
npm test -- src/__tests__/WalletConnector.test.tsx
```

### Context Tests

Context tests verify state management:

- **File**: `src/__tests__/WalletContext.test.tsx`
- **Coverage**:
  - Restores key from Freighter on mount
  - Returns null key when Freighter errors
  - `connect()` sets publicKey
  - `disconnect()` clears publicKey

Run with:

```bash
npm test -- src/__tests__/WalletContext.test.tsx
```

### E2E Tests

E2E tests verify the full wallet flow in a real browser:

- **File**: `e2e/tests/wallet-connection.spec.ts`
- **Coverage**:
  - User sees "Connect Freighter" button
  - Clicking button shows install modal (if Freighter not installed)
  - Wallet connection flow (with mock Freighter)
  - Balance display updates after connection

Run with:

```bash
cd e2e
npm install
npx playwright install
npx playwright test
```

### Manual Testing with Freighter

1. **Install Freighter**: [Chrome](https://chrome.google.com/webstore/detail/freighter/bcacfldlkkdogcmkkibnjlakofdplcbk) or [Firefox](https://addons.mozilla.org/en-US/firefox/addon/freighter/)
2. **Create Test Account**: Generate a new Freighter wallet.
3. **Fund Testnet Account**: Use [Stellar Testnet Friendbot](https://developers.stellar.org/docs/tutorials/create-account#fund-with-the-friendbot) to add 10,000 XLM.
4. **Configure App**: Set `NEXT_PUBLIC_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"`.
5. **Switch Freighter to Testnet**: In Freighter extension settings, select "Testnet".
6. **Start App**: `npm run dev` from the frontend directory.
7. **Test Connection**: Click "Connect Freighter" and approve the connection.
8. **Test Transaction**: Attempt to claim a reward and approve the transaction in Freighter.

---

## Troubleshooting

### Freighter Popup Does Not Appear

**Symptoms**: User clicks "Connect" or tries to sign, but no Freighter popup appears.

**Causes**:

1. Freighter is not installed.
2. Freighter is disabled/suspended.
3. PopupError: Browser is blocking popups.

**Solutions**:

- Check `freighter.isConnected()` returns `true`.
- Ask user to enable Freighter in extension settings.
- Ask user to disable popup blockers for the app domain.

### "Freighter extension not installed" Error

**Symptoms**: Error message appears even though Freighter is installed.

**Causes**:

1. Freighter injection failed (e.g., race condition during page load).
2. Browser does not support the extension (e.g., Safari without special support).

**Solutions**:

- Reload the page.
- Check that Freighter is enabled in extension settings.
- Use a Chrome or Firefox-based browser.

### Transaction Silently Fails (Gets Stuck "Pending")

**Symptoms**: User signs a transaction but it never confirms or fails.

**Causes**:

1. Network mismatch: Freighter on different network than app.
2. Soroban RPC endpoint is unreachable or misconfigured.
3. Transaction hash mismatch in polling.

**Solutions**:

- Verify Freighter network matches `NEXT_PUBLIC_NETWORK_PASSPHRASE`.
- Check RPC endpoint is reachable: `curl $NEXT_PUBLIC_SOROBAN_RPC_URL`.
- Check app logs for polling errors.

### "Network mismatch" or similar RPC error after signing

**Symptoms**: Freighter shows transaction was signed, but RPC rejects it immediately.

**Causes**:

1. Freighter on Testnet, app configured for Mainnet (or vice versa).
2. Wrong network passphrase in `NEXT_PUBLIC_NETWORK_PASSPHRASE`.

**Solutions**:

- Verify environment variable matches the intended network.
- Switch Freighter to the correct network.
- Reload the app and try again.

### Tests Fail with "Cannot find module @stellar/freighter-api"

**Symptoms**: Jest test fails because Freighter mock is not loaded.

**Causes**:

1. Jest `moduleNameMapper` is not configured correctly.
2. Test file does not match `testMatch` pattern.

**Solutions**:

- Verify `jest.config.js` maps `@stellar/freighter-api` to `src/__mocks__/freighter.ts`.
- Ensure test file ends with `.test.tsx` (not `.test.ts`).

---

## Contributing and Best Practices

### Adding a New Contract Call

When adding a new contract interaction (e.g., new campaign feature):

1. **Define the contract method** in `src/lib/soroban.ts`:

   ```typescript
   export async function myNewAction(publicKey: string, arg1: string) {
     return invokeContract(publicKey, REWARDS_CONTRACT_ID, "my_new_action", [
       new Address(publicKey).toScVal(),
       nativeToScVal(arg1, { type: "string" }),
     ]);
   }
   ```

2. **Handle errors** in the calling component:

   ```typescript
   try {
     await myNewAction(publicKey, arg1);
   } catch (error) {
     const parsed = SorobanErrorParser.parse(error);
     toast(parsed.userMessage, "error");
   }
   ```

3. **Add unit tests** for the new contract call using the mocked Freighter API.

4. **Test with real Freighter** on Testnet before deployment.

### Avoiding Common Pitfalls

- **Do not store private keys or seed phrases** in the app. Freighter handles key management.
- **Always simulate before signing** to catch validation errors early.
- **Do not hardcode network passphrases**; use environment variables.
- **Poll for transaction confirmation** instead of assuming success after signing.
- **Handle network mismatches gracefully** with clear error messages.

---

## Resources

- [Freighter Documentation](https://developers.stellar.org/docs/tools-and-sdks/wallet-guide)
- [Stellar SDK for JavaScript](https://github.com/stellar/js-stellar-sdk)
- [Soroban Smart Contracts](https://developers.stellar.org/docs/learn/soroban)
- [Stellar Testnet Friendbot](https://developers.stellar.org/docs/tutorials/create-account#fund-with-the-friendbot)

---

## Questions?

For questions or issues related to Freighter integration, open an issue on the [project repository](https://github.com/Dev-Odun-oss/Soroban-Loyalty).
