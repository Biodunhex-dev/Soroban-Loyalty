import request from "supertest";
import { createApp } from "../../app";
import { explainRewardsByUserQuery } from "../../services/reward.service";
import {
  SEEDED_USER_ADDRESS,
  setupIntegrationDatabase,
  teardownIntegrationDatabase,
} from "./testDb";

describe("GET endpoints integration", () => {
  const app = createApp();

  beforeAll(async () => {
    await setupIntegrationDatabase();
  });

  afterAll(async () => {
    await teardownIntegrationDatabase();
  });

  it("GET /health returns API health payload", async () => {
    const response = await request(app).get("/health");

    expect(response.status).toBe(200);
    expect(["healthy", "degraded", "unhealthy"]).toContain(response.body.status);
    expect(response.body.checks).toHaveProperty("database");
    expect(response.body.checks).toHaveProperty("stellar");
  });

  it("GET /metrics returns Prometheus metrics text", async () => {
    const response = await request(app).get("/metrics");

    expect(response.status).toBe(200);
    expect(response.headers["content-type"]).toContain("text/plain");
    expect(response.text).toContain("http_requests_total");
  });

  it("GET /campaigns returns seeded campaigns", async () => {
    const response = await request(app).get("/campaigns?limit=10&offset=0");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(2);
    expect(response.body.campaigns).toHaveLength(2);
    expect(response.body.campaigns[0]).toHaveProperty("merchant", SEEDED_USER_ADDRESS);
  });

  it("GET /campaigns?search= filters by name substring (case-insensitive)", async () => {
    const response = await request(app).get("/campaigns?search=summer");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("name", "Summer Sale");
  });

  it("GET /campaigns?status=active returns only active campaigns", async () => {
    const response = await request(app).get("/campaigns?status=active");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("active", true);
  });

  it("GET /campaigns?status=inactive returns only inactive campaigns", async () => {
    const response = await request(app).get("/campaigns?status=inactive");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("active", false);
  });

  it("GET /campaigns?expires_before= filters by expiration upper bound", async () => {
    const response = await request(app).get("/campaigns?expires_before=1500000000");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("name", "Winter Promo");
  });

  it("GET /campaigns?expires_after= filters by expiration lower bound", async () => {
    const response = await request(app).get("/campaigns?expires_after=1500000000");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("name", "Summer Sale");
  });

  it("GET /campaigns composes search + status filters", async () => {
    const response = await request(app).get("/campaigns?search=winter&status=inactive");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(1);
    expect(response.body.campaigns[0]).toHaveProperty("name", "Winter Promo");
  });

  it("GET /campaigns returns empty when no campaigns match filters", async () => {
    const response = await request(app).get("/campaigns?search=nonexistent");

    expect(response.status).toBe(200);
    expect(response.body.total).toBe(0);
    expect(response.body.campaigns).toHaveLength(0);
  });

  it("GET /campaigns/:id returns one campaign for valid id", async () => {
    const response = await request(app).get("/campaigns/1");

    expect(response.status).toBe(200);
    expect(response.body.campaign).toHaveProperty("id", "1");
  });

  it("GET /campaigns/:id returns 400 for invalid id", async () => {
    const response = await request(app).get("/campaigns/not-a-number");

    expect(response.status).toBe(400);
    expect(response.body.error).toMatch(/invalid id/i);
  });

  it("GET /campaigns/:id returns 404 for unknown id", async () => {
    const response = await request(app).get("/campaigns/999999");

    expect(response.status).toBe(404);
    expect(response.body.error).toMatch(/not found/i);
  });

  it("GET /user/:address/rewards returns rewards for valid address", async () => {
    const response = await request(app).get(`/user/${SEEDED_USER_ADDRESS}/rewards`);

    expect(response.status).toBe(200);
    expect(response.body.rewards).toHaveLength(1);
    expect(response.body.rewards[0]).toHaveProperty("campaign_id", "1");
  });

  it("GET /user/:address/rewards returns 400 for invalid address", async () => {
    const response = await request(app).get("/user/invalid-address/rewards");

    expect(response.status).toBe(400);
    expect(response.body.error).toMatch(/invalid stellar address/i);
  });

  it("GET /analytics returns aggregate data for valid days", async () => {
    const response = await request(app).get("/analytics?days=30");

    expect(response.status).toBe(200);
    expect(response.body).toHaveProperty("totalClaims");
    expect(response.body).toHaveProperty("totalLYT");
    expect(response.body).toHaveProperty("claimsPerCampaign");
    expect(response.body).toHaveProperty("claimsOverTime");
  });

  it("GET /analytics clamps invalid days and still returns data", async () => {
    const response = await request(app).get("/analytics?days=-5");

    expect(response.status).toBe(200);
    expect(response.body).toHaveProperty("totalClaims");
    expect(response.body.totalClaims).toBeGreaterThanOrEqual(0);
  });

  it("verifies rewards query plan via EXPLAIN ANALYZE", async () => {
    const queryPlan = await explainRewardsByUserQuery(SEEDED_USER_ADDRESS);
    const joinedPlan = queryPlan.join("\n");

    expect(joinedPlan).toContain("campaigns");
    expect(joinedPlan).toMatch(/Index Scan|Bitmap Heap Scan|Seq Scan/);
  });
});
