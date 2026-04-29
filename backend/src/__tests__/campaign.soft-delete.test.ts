import {
  getCampaigns,
  getCampaignById,
  softDeleteCampaign,
  restoreCampaign,
} from "../services/campaign.service";

jest.mock("../db", () => ({
  pool: { query: jest.fn() },
}));

// eslint-disable-next-line @typescript-eslint/no-var-requires
const mockQuery: jest.Mock = require("../db").pool.query;

const baseCampaign = {
  id: 1,
  merchant: "GABC",
  name: "Summer Sale",
  reward_amount: 100,
  expiration: 9999999999,
  active: true,
  total_claimed: 0,
  display_order: 0,
  created_at: new Date(),
  deleted_at: null,
};

function mockCampaignQuery(rows = [baseCampaign], count = "1") {
  mockQuery
    .mockResolvedValueOnce({ rows })
    .mockResolvedValueOnce({ rows: [{ count }] });
}

describe("campaign soft delete", () => {
  afterEach(() => jest.clearAllMocks());

  it("getCampaigns filters deleted_at IS NULL", async () => {
    mockCampaignQuery();

    const result = await getCampaigns();

    expect(mockQuery).toHaveBeenCalledTimes(2);
    const [listSql] = mockQuery.mock.calls[0];
    expect(listSql).toContain("deleted_at IS NULL");
    const [countSql] = mockQuery.mock.calls[1];
    expect(countSql).toContain("deleted_at IS NULL");
    expect(result.total).toBe(1);
  });

  it("getCampaignById filters deleted_at IS NULL", async () => {
    mockQuery.mockResolvedValueOnce({ rows: [baseCampaign] });

    await getCampaignById(1);

    const [sql] = mockQuery.mock.calls[0];
    expect(sql).toContain("deleted_at IS NULL");
  });

  it("getCampaignById returns null for soft-deleted campaign", async () => {
    mockQuery.mockResolvedValueOnce({ rows: [] });

    const result = await getCampaignById(1);

    expect(result).toBeNull();
  });

  it("softDeleteCampaign sets deleted_at and returns true", async () => {
    mockQuery.mockResolvedValueOnce({ rowCount: 1 });

    const result = await softDeleteCampaign(1);

    expect(result).toBe(true);
    const [sql] = mockQuery.mock.calls[0];
    expect(sql).toContain("SET deleted_at = NOW()");
    expect(sql).toContain("deleted_at IS NULL");
  });

  it("softDeleteCampaign returns false when campaign not found", async () => {
    mockQuery.mockResolvedValueOnce({ rowCount: 0 });

    const result = await softDeleteCampaign(99);

    expect(result).toBe(false);
  });

  it("restoreCampaign clears deleted_at and returns true", async () => {
    mockQuery.mockResolvedValueOnce({ rowCount: 1 });

    const result = await restoreCampaign(1);

    expect(result).toBe(true);
    const [sql] = mockQuery.mock.calls[0];
    expect(sql).toContain("SET deleted_at = NULL");
    expect(sql).toContain("deleted_at IS NOT NULL");
  });

  it("restoreCampaign returns false when campaign not soft-deleted", async () => {
    mockQuery.mockResolvedValueOnce({ rowCount: 0 });

    const result = await restoreCampaign(1);

    expect(result).toBe(false);
  });
});

describe("getCampaigns filters", () => {
  afterEach(() => jest.clearAllMocks());

  it("applies search filter with ILIKE", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { search: "summer" });

    const [listSql, listParams] = mockQuery.mock.calls[0];
    expect(listSql).toContain("ILIKE");
    expect(listParams).toContain("%summer%");
  });

  it("applies status=active filter", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { status: "active" });

    const [listSql, listParams] = mockQuery.mock.calls[0];
    expect(listSql).toContain("active =");
    expect(listParams).toContain(true);
  });

  it("applies status=inactive filter", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { status: "inactive" });

    const [, listParams] = mockQuery.mock.calls[0];
    expect(listParams).toContain(false);
  });

  it("applies expires_before filter", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { expires_before: 2000000000 });

    const [listSql, listParams] = mockQuery.mock.calls[0];
    expect(listSql).toContain("expiration <=");
    expect(listParams).toContain(2000000000);
  });

  it("applies expires_after filter", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { expires_after: 1000000000 });

    const [listSql, listParams] = mockQuery.mock.calls[0];
    expect(listSql).toContain("expiration >=");
    expect(listParams).toContain(1000000000);
  });

  it("composes multiple filters", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { search: "sale", status: "active", expires_after: 1000000000 });

    const [listSql, listParams] = mockQuery.mock.calls[0];
    expect(listSql).toContain("ILIKE");
    expect(listSql).toContain("active =");
    expect(listSql).toContain("expiration >=");
    expect(listParams).toContain("%sale%");
    expect(listParams).toContain(true);
    expect(listParams).toContain(1000000000);
  });

  it("uses same params for list and count queries", async () => {
    mockCampaignQuery();

    await getCampaigns(20, 0, { search: "promo", status: "inactive" });

    const [, listParams] = mockQuery.mock.calls[0];
    const [, countParams] = mockQuery.mock.calls[1];
    // count params should not include limit/offset
    expect(countParams).toEqual(listParams.slice(0, -2));
  });
});
