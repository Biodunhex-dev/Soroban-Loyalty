"use client";

import { useState } from "react";
import { useWallet } from "@/context/WalletContext";
import { Tooltip } from "@/components/Tooltip";

function FreighterModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="freighter-modal-title">
      <div className="modal">
        <h2 id="freighter-modal-title">Freighter Wallet Required</h2>
        <p>
          Freighter is a browser extension that lets you sign Stellar transactions securely.
          Install it to connect your wallet and interact with SorobanLoyalty.
        </p>
        <div className="modal-actions">
          <a
            href="https://chrome.google.com/webstore/detail/freighter/bcacfldlkkdogcmkkibnjlakofdplcbk"
            target="_blank"
            rel="noopener noreferrer"
            className="btn btn-primary"
          >
            Install for Chrome
          </a>
          <a
            href="https://addons.mozilla.org/en-US/firefox/addon/freighter/"
            target="_blank"
            rel="noopener noreferrer"
            className="btn btn-secondary"
          >
            Install for Firefox
          </a>
          <button onClick={onClose} className="btn btn-outline">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

export function WalletConnector() {
  const { publicKey, connecting, lytBalance, balanceLoading, connect, disconnect } = useWallet();
  const [showModal, setShowModal] = useState(false);

  const handleConnect = async () => {
    // Detect Freighter: the extension injects window.freighter
    const hasFreighter =
      typeof window !== "undefined" &&
      // @ts-expect-error freighter is injected by the extension
      (typeof window.freighter !== "undefined" || typeof window.freighterApi !== "undefined");

    if (!hasFreighter) {
      setShowModal(true);
      return;
    }
    await connect();
  };

  if (publicKey) {
    return (
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            fontSize: 13,
            color: "var(--text-secondary)",
          }}
        >
          {balanceLoading ? <span className="inline-spinner" aria-hidden="true" /> : null}
          <Tooltip content="Your LYT token balance — earned by claiming campaigns">
            <span aria-live="polite" aria-busy={balanceLoading}>
              {lytBalance.toLocaleString()} LYT
            </span>
          </Tooltip>
        </span>
        <span style={{ fontFamily: "monospace", fontSize: 13 }}>
          {publicKey.slice(0, 6)}…{publicKey.slice(-4)}
        </span>
        <button onClick={disconnect} className="btn btn-outline">
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <>
      <button onClick={handleConnect} disabled={connecting} className="btn btn-primary">
        {connecting ? "Connecting…" : "Connect Freighter"}
      </button>
      {showModal && <FreighterModal onClose={() => setShowModal(false)} />}
    </>
  );
}
