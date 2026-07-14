import DemoShell from "./demo-shell";

// The whole /demo subtree is public (middleware.ts) and renders fixture data.
// It owns its chrome — the operator nav is suppressed here (app/chrome.tsx),
// because every link in it would bounce a visitor to the passcode screen.
export default function DemoLayout({ children }: { children: React.ReactNode }) {
  return <DemoShell>{children}</DemoShell>;
}
