import { render, screen, act, waitFor } from "@testing-library/react";
import { WalletProvider, useWallet } from "@/context/WalletContext";
import freighter from "@stellar/freighter-api";
import React from "react";

const WALLET_STORAGE_KEY = "soroban_wallet_public_key";

const Probe = () => {
  const { publicKey, connecting, connect, disconnect } = useWallet();
  return (
    <div>
      <span data-testid="key">{publicKey ?? "none"}</span>
      <span data-testid="connecting">{String(connecting)}</span>
      <button onClick={connect}>connect</button>
      <button onClick={disconnect}>disconnect</button>
    </div>
  );
};

const setup = () =>
  render(
    <WalletProvider>
      <Probe />
    </WalletProvider>
  );

beforeEach(() => {
  jest.clearAllMocks();
  localStorage.clear();
});

test("auto-reconnects from persisted key on mount", async () => {
  localStorage.setItem(WALLET_STORAGE_KEY, "GAUTO");
  (freighter.getPublicKey as jest.Mock).mockResolvedValue({ publicKey: "GAUTO", error: null });
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("GAUTO"));
});

test("does not attempt auto-reconnect when no persisted key exists", async () => {
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("none"));
  expect(freighter.getPublicKey).not.toHaveBeenCalled();
});

test("silently fails auto-reconnect when freighter session is invalid", async () => {
  localStorage.setItem(WALLET_STORAGE_KEY, "GAUTO");
  (freighter.getPublicKey as jest.Mock).mockResolvedValue({ publicKey: "", error: "locked" });
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("none"));
  expect(localStorage.getItem(WALLET_STORAGE_KEY)).toBeNull();
});

test("connect sets publicKey", async () => {
  (freighter.isConnected as jest.Mock).mockResolvedValue(true);
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("none"));

  (freighter.getPublicKey as jest.Mock).mockResolvedValue({ publicKey: "GNEW", error: null });
  await act(async () => { screen.getByText("connect").click(); });
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("GNEW"));
  expect(localStorage.getItem(WALLET_STORAGE_KEY)).toBe("GNEW");
});

test("disconnect clears publicKey", async () => {
  localStorage.setItem(WALLET_STORAGE_KEY, "GAUTO");
  (freighter.getPublicKey as jest.Mock).mockResolvedValue({ publicKey: "GAUTO", error: null });
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("GAUTO"));
  act(() => { screen.getByText("disconnect").click(); });
  expect(screen.getByTestId("key")).toHaveTextContent("none");
  expect(localStorage.getItem(WALLET_STORAGE_KEY)).toBeNull();
});

test("syncs wallet state across tabs via storage events", async () => {
  setup();
  await waitFor(() => expect(screen.getByTestId("key")).toHaveTextContent("none"));

  act(() => {
    window.dispatchEvent(new StorageEvent("storage", {
      key: WALLET_STORAGE_KEY,
      newValue: "GTABSYNC",
    }));
  });

  expect(screen.getByTestId("key")).toHaveTextContent("GTABSYNC");

  act(() => {
    window.dispatchEvent(new StorageEvent("storage", {
      key: WALLET_STORAGE_KEY,
      newValue: null,
    }));
  });

  expect(screen.getByTestId("key")).toHaveTextContent("none");
});
