import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { CONTRACT_VERSION } from "./shared/contracts";

function FoundationShell() {
  return (
    <main>
      <h1>Luno</h1>
      <p>Desktop migration foundation</p>
      <small>IPC contract v{CONTRACT_VERSION}</small>
    </main>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("Luno root element is missing");

createRoot(root).render(
  <StrictMode>
    <FoundationShell />
  </StrictMode>,
);
