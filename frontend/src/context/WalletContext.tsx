"use client";

import React, { createContext, useCallback, useContext, useEffect, useState } from "react";
import { connectWallet, getPublicKey } from "@/lib/freighter";
import { api } from "@/lib/api";
import { useToast } from "@/context/ToastContext";

const WALLET_STORAGE_KEY = "soroban_wallet_public_key";

interface WalletCtx {
  publicKey: string | null;
  connecting: boolean;
  mounted: boolean;
  lytBalance: number;
  balanceLoading: boolean;
  connect: () => Promise<void>;
  disconnect: () => void;
  refreshBalance: () => Promise<void>;
}

export const WalletContext = createContext<WalletCtx>({
  publicKey: null,
  connecting: false,
  mounted: false,
  lytBalance: 0,
  balanceLoading: false,
  connect: async () => {},
  disconnect: () => {},
  refreshBalance: async () => {},
});

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const { toast } = useToast();
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [lytBalance, setLytBalance] = useState(0);
  const [balanceLoading, setBalanceLoading] = useState(false);
  // `mounted` is false on the server and during the first render,
  // preventing any wallet-dependent UI from rendering until the client
  // has hydrated — eliminating the hydration mismatch.
  const [mounted, setMounted] = useState(false);
  const [isPageVisible, setIsPageVisible] = useState(true);

  const refreshBalance = useCallback(async () => {
    if (!publicKey) {
      setLytBalance(0);
      setBalanceLoading(false);
      return;
    }

    setBalanceLoading(true);
    try {
      const { rewards } = await api.getUserRewards(publicKey);
      const availableBalance = rewards.reduce((sum, reward) => {
        const remaining = reward.redeemed ? reward.amount - reward.redeemed_amount : reward.amount;
        return sum + Math.max(remaining, 0);
      }, 0);
      setLytBalance(availableBalance);
    } catch (error) {
      console.error("Failed to refresh LYT balance", error);
    } finally {
      setBalanceLoading(false);
    }
  }, [publicKey]);

  useEffect(() => {
    setMounted(true);

    if (typeof window === "undefined") return;

    const storedKey = localStorage.getItem(WALLET_STORAGE_KEY);
    if (!storedKey) return;

    void getPublicKey().then((authorizedKey) => {
      // Silent failure path (e.g. Freighter locked): do not toast.
      if (!authorizedKey) {
        setPublicKey(null);
        localStorage.removeItem(WALLET_STORAGE_KEY);
        return;
      }

      setPublicKey(authorizedKey);
      if (authorizedKey !== storedKey) {
        localStorage.setItem(WALLET_STORAGE_KEY, authorizedKey);
      }
    });
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;

    const onStorage = (event: StorageEvent) => {
      if (event.key !== WALLET_STORAGE_KEY) return;
      setPublicKey(event.newValue || null);
    };

    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  useEffect(() => {
    if (typeof document === "undefined") return;
    const onVisibilityChange = () => {
      setIsPageVisible(document.visibilityState === "visible");
    };
    onVisibilityChange();
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => document.removeEventListener("visibilitychange", onVisibilityChange);
  }, []);

  useEffect(() => {
    if (!publicKey) {
      setLytBalance(0);
      setBalanceLoading(false);
      return;
    }
    void refreshBalance();
  }, [publicKey, refreshBalance]);

  useEffect(() => {
    if (!publicKey || !isPageVisible) return;
    const interval = window.setInterval(() => {
      void refreshBalance();
    }, 30_000);
    return () => window.clearInterval(interval);
  }, [publicKey, isPageVisible, refreshBalance]);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      const key = await connectWallet();
      setPublicKey(key);
      if (typeof window !== "undefined") {
        localStorage.setItem(WALLET_STORAGE_KEY, key);
      }
      toast(`Wallet connected: ${key.slice(0, 6)}...${key.slice(-4)}`, 'success');
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to connect wallet';
      toast(errorMessage, 'error');
      console.error('Wallet connection error:', error);
    } finally {
      setConnecting(false);
    }
  }, [toast]);

  const disconnect = useCallback(() => {
    setPublicKey(null);
    if (typeof window !== "undefined") {
      localStorage.removeItem(WALLET_STORAGE_KEY);
    }
    toast('Wallet disconnected', 'info');
  }, [toast]);

  return (
    <WalletContext.Provider
      value={{
        publicKey,
        connecting,
        mounted,
        lytBalance,
        balanceLoading,
        connect,
        disconnect,
        refreshBalance,
      }}
    >
      {children}
    </WalletContext.Provider>
  );
}

export const useWallet = () => useContext(WalletContext);
