import { render, screen, fireEvent } from "./test-utils";
import { OnboardingModal } from "@/components/OnboardingModal";
import { Tooltip } from "@/components/Tooltip";
import { act } from "@testing-library/react";

// ── OnboardingModal ────────────────────────────────────────────────────────

describe("OnboardingModal", () => {
  beforeEach(() => {
    localStorage.clear();
    jest.useFakeTimers();
  });
  afterEach(() => jest.useRealTimers());

  test("shows modal for first-time visitors", () => {
    render(<OnboardingModal />);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText(/Welcome to SorobanLoyalty/i)).toBeInTheDocument();
  });

  test("does not show modal when already completed", () => {
    localStorage.setItem("soroban_onboarding_done", "1");
    render(<OnboardingModal />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("shows progress indicator with 3 dots", () => {
    render(<OnboardingModal />);
    const progress = screen.getByLabelText(/Step 1 of 3/i);
    expect(progress).toBeInTheDocument();
  });

  test("Skip button dismisses modal and persists to localStorage", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /skip/i }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem("soroban_onboarding_done")).toBe("1");
  });

  test("Next advances to step 2", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText(/Connect Your Wallet/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Step 2 of 3/i)).toBeInTheDocument();
  });

  test("Next advances to step 3", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByText(/Claim Your First Reward/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Step 3 of 3/i)).toBeInTheDocument();
  });

  test("last step shows Get Started button", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(screen.getByRole("button", { name: /get started/i })).toBeInTheDocument();
  });

  test("Get Started dismisses modal and persists to localStorage", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    fireEvent.click(screen.getByRole("button", { name: /get started/i }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem("soroban_onboarding_done")).toBe("1");
  });

  test("step 2 contains Freighter install link", () => {
    render(<OnboardingModal />);
    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    const link = screen.getByRole("link", { name: /freighter browser extension/i });
    expect(link).toHaveAttribute("href", expect.stringContaining("chrome.google.com"));
    expect(link).toHaveAttribute("target", "_blank");
  });
});

// ── Tooltip ────────────────────────────────────────────────────────────────

describe("Tooltip", () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  test("tooltip not visible initially", () => {
    render(
      <Tooltip content="Helpful hint">
        <button>Hover me</button>
      </Tooltip>
    );
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  test("tooltip appears after 300ms on mouseenter", () => {
    render(
      <Tooltip content="Helpful hint">
        <button>Hover me</button>
      </Tooltip>
    );
    fireEvent.mouseEnter(screen.getByRole("button"));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    act(() => jest.advanceTimersByTime(300));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Helpful hint");
  });

  test("tooltip disappears on mouseleave", () => {
    render(
      <Tooltip content="Helpful hint">
        <button>Hover me</button>
      </Tooltip>
    );
    fireEvent.mouseEnter(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    fireEvent.mouseLeave(screen.getByRole("button"));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  test("tooltip appears on focus", () => {
    render(
      <Tooltip content="Focus hint">
        <button>Focus me</button>
      </Tooltip>
    );
    fireEvent.focus(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Focus hint");
  });

  test("tooltip disappears on blur", () => {
    render(
      <Tooltip content="Focus hint">
        <button>Focus me</button>
      </Tooltip>
    );
    fireEvent.focus(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    fireEvent.blur(screen.getByRole("button"));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  test("trigger has aria-describedby pointing to tooltip when visible", () => {
    render(
      <Tooltip content="Aria hint">
        <button>Trigger</button>
      </Tooltip>
    );
    fireEvent.mouseEnter(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    const tooltip = screen.getByRole("tooltip");
    const trigger = screen.getByRole("button");
    expect(trigger).toHaveAttribute("aria-describedby", tooltip.id);
  });

  test("tooltip not shown if mouse leaves before delay", () => {
    render(
      <Tooltip content="Quick leave">
        <button>Hover me</button>
      </Tooltip>
    );
    fireEvent.mouseEnter(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(100));
    fireEvent.mouseLeave(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  test("respects custom delay", () => {
    render(
      <Tooltip content="Slow hint" delay={500}>
        <button>Hover me</button>
      </Tooltip>
    );
    fireEvent.mouseEnter(screen.getByRole("button"));
    act(() => jest.advanceTimersByTime(300));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    act(() => jest.advanceTimersByTime(200));
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
  });
});
