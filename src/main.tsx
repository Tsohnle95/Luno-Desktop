import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { CONTRACT_VERSION } from "./shared/contracts";
import { Shell } from "./app/Shell";

const root = document.getElementById("root");
if (!root) throw new Error("Luno root element is missing");

createRoot(root).render(
  <StrictMode>
    <Shell player={<small>IPC contract v{CONTRACT_VERSION}</small>} />
  </StrictMode>,
);
