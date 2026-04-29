import { Router, Request, Response } from "express";
import { getAnalytics } from "../services/analytics.service";
import { asyncHandler } from "../middleware/errorHandler";

export const analyticsRouter = Router();

/**
 * @openapi
 * /analytics:
 *   get:
 *     summary: Get platform analytics
 *     description: Returns aggregated statistics about claims and rewards over a specified period.
 *     tags: [Analytics]
 *     parameters:
 *       - in: query
 *         name: days
 *         schema:
 *           type: integer
 *           minimum: 1
 *           maximum: 365
 *           default: 30
 *         description: Number of days to look back.
 *     responses:
 *       200:
 *         description: Analytics data.
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/AnalyticsData'
 *       500:
 *         description: Server error.
 */

analyticsRouter.get("/", async (req: Request, res: Response) => {
  const days = Math.min(Math.max(parseInt(req.query.days as string, 10) || 30, 1), 365);
  const data = await getAnalytics(days);
  res.json(data);
});
