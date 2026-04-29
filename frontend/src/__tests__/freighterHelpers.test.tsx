import freighter from "@stellar/freighter-api";
import { connectWallet, getPublicKey, signTransaction } from "@/lib/freighter";

describe("Freighter helper wrapper", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  test("connectWallet resolves when Freighter is installed and authorized", async () => {
    (freighter.isConnected as jest.Mock).mockResolvedValue(true);
    (freighter.getPublicKey as jest.Mock).mockResolvedValue({
      publicKey: "GTEST123",
      error: null,
    });

    await expect(connectWallet()).resolves.toBe("GTEST123");
    expect(freighter.isConnected).toHaveBeenCalledTimes(1);
    expect(freighter.getPublicKey).toHaveBeenCalledTimes(1);
  });

  test("connectWallet throws when Freighter is not installed", async () => {
    (freighter.isConnected as jest.Mock).mockResolvedValue(false);

    await expect(connectWallet()).rejects.toThrow(
      "Freighter extension not installed",
    );
    expect(freighter.isConnected).toHaveBeenCalledTimes(1);
  });

  test("getPublicKey returns null when Freighter returns an error", async () => {
    (freighter.getPublicKey as jest.Mock).mockResolvedValue({
      publicKey: "",
      error: "no key",
    });

    await expect(getPublicKey()).resolves.toBeNull();
    expect(freighter.getPublicKey).toHaveBeenCalledTimes(1);
  });

  test("signTransaction resolves signed XDR on success", async () => {
    (freighter.signTransaction as jest.Mock).mockResolvedValue({
      signedTxXdr: "signed-xdr",
      error: null,
    });

    await expect(
      signTransaction("unsigned-xdr", "Test SDF Network ; September 2015"),
    ).resolves.toBe("signed-xdr");
    expect(freighter.signTransaction).toHaveBeenCalledWith("unsigned-xdr", {
      networkPassphrase: "Test SDF Network ; September 2015",
    });
  });

  test("signTransaction throws when Freighter rejects the transaction", async () => {
    (freighter.signTransaction as jest.Mock).mockResolvedValue({
      signedTxXdr: "",
      error: "user rejected",
    });

    await expect(
      signTransaction("unsigned-xdr", "Test SDF Network ; September 2015"),
    ).rejects.toThrow("user rejected");
    expect(freighter.signTransaction).toHaveBeenCalledTimes(1);
  });
});
