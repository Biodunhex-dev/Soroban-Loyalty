import { Router, Request, Response } from "express";
import { z } from "zod";
import { createRewardClaim, DuplicateClaimError, getRewardsByUser } from "../services/reward.service";
import { asyncHandler } from "../middleware/errorHandler";
import { BadRequestError } from "../utils/errors";

export const rewardRouter = Router();
const ClaimSchema = z.object({
  campaign_id: z.number().int().positive(),
  amount: z.number().int().positive(),
});

/**
 * @openapi
 * /user/{address}/rewards:
 *   get:
 *     summary: Get user rewards
 *     description: Returns all rewards associated with a specific Stellar address.
 *     tags: [Rewards]
 *     parameters:
 *       - in: path
 *         name: address
 *         required: true
 *         schema:
 *           type: string
 *           minLength: 56
 *           maxLength: 56
 *         description: The 56-character Stellar public key.
 *     responses:
 *       200:
 *         description: A list of rewards.
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 rewards:
 *                   type: array
 *                   items:
 *                     $ref: '#/components/schemas/Reward'
 *       400:
 *         description: Invalid Stellar address.
 *       500:
 *         description: Server error.
 */
rewardRouter.get("/user/:address/rewards", asyncHandler(async (req: Request, res: Response) => {
  const address = String(req.params.address);
  if (!address || address.length !== 56) {
    throw new BadRequestError("Invalid Stellar address", { address });
  }
  try {
    const rewards = await getRewardsByUser(address);
    res.json({ rewards });
  } catch (err) {
    res.status(500).json({ error: "Failed to fetch rewards" });
  }
}));

/**
 * POST /user/:address/rewards/claim
 * Inserts a reward claim once per (user, campaign). Duplicate claims return 409.
 */
rewardRouter.post("/user/:address/rewards/claim", async (req: Request, res: Response) => {
  const address = String(req.params.address);
  if (!address || address.length !== 56) {
    return res.status(400).json({ error: "Invalid Stellar address" });
  }

  const parsed = ClaimSchema.safeParse(req.body);
  if (!parsed.success) {
    return res.status(400).json({ error: "campaign_id and amount must be positive integers" });
  }

  try {
    await createRewardClaim({
      user_address: address,
      campaign_id: parsed.data.campaign_id,
      amount: parsed.data.amount,
      redeemed: false,
      redeemed_amount: 0,
    });
    return res.status(201).json({ ok: true });
  } catch (err) {
    if (err instanceof DuplicateClaimError) {
      return res.status(409).json({ error: err.message });
    }
    return res.status(500).json({ error: "Failed to claim reward" });
  }
});
