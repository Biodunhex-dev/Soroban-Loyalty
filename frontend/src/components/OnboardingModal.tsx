"use client";

import { useEffect, useState } from "react";
import { FocusTrap } from "@/components/FocusTrap";

const STORAGE_KEY = "soroban_onboarding_done";

const STEPS = [
  {
    title: "Welcome to SorobanLoyalty",
    illustration: "🌟",
    body: "An on-chain loyalty platform built on Stellar. Businesses create reward campaigns and you earn LYT tokens — stored transparently on the blockchain.",
  },
  {
    title: "Connect Your Wallet",
    illustration: "🔗",
    body: (
      <>
        You need the{" "}
        <a
          href="https://chrome.google.com/webstore/detail/freighter/bcacfldlkkdogcmkkibnjlakofdplcbk"
          target="_blank"
          rel="noopener noreferrer"
          className="onboarding-link"
        >
          Freighter browser extension
        </a>{" "}
        to sign Stellar transactions. Install it, then click <strong>Connect Freighter</strong> in the header.
      </>
    ),
  },
  {
    title: "Claim Your First Reward",
    illustration: "🎁",
    body: "Browse active campaigns on the Dashboard. Click Claim on any campaign to earn LYT tokens directly to your wallet. Redeem them any time.",
  },
] as const;

export function OnboardingModal() {
  const [step, setStep] = useState(0);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (!localStorage.getItem(STORAGE_KEY)) setVisible(true);
  }, []);

  const dismiss = () => {
    localStorage.setItem(STORAGE_KEY, "1");
    setVisible(false);
  };

  const next = () => {
    if (step < STEPS.length - 1) setStep((s) => s + 1);
    else dismiss();
  };

  if (!visible) return null;

  const current = STEPS[step];
  const isLast = step === STEPS.length - 1;

  return (
    <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
      <FocusTrap active>
        <div className="modal onboarding-modal">
          {/* Progress dots */}
          <div className="onboarding-progress" aria-label={`Step ${step + 1} of ${STEPS.length}`}>
            {STEPS.map((_, i) => (
              <span
                key={i}
                className={`onboarding-dot${i === step ? " onboarding-dot--active" : ""}`}
                aria-hidden="true"
              />
            ))}
          </div>

          <div className="onboarding-illustration" aria-hidden="true">
            {current.illustration}
          </div>

          <h2 id="onboarding-title" className="onboarding-title">
            {current.title}
          </h2>
          <p className="onboarding-body">{current.body}</p>

          <div className="modal-actions">
            <button onClick={dismiss} className="btn btn-outline">
              Skip
            </button>
            <button onClick={next} className="btn btn-primary">
              {isLast ? "Get Started" : "Next"}
            </button>
          </div>
        </div>
      </FocusTrap>
    </div>
  );
}
