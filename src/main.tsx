import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { CONTRACT_VERSION } from "./shared/contracts";
import { Shell } from "./app/Shell";
import { bridge } from "./app/bridge";
import { createBackendProjection, subscribeBackendProjection } from "./state/backend_projection";

const root = document.getElementById("root");
if (!root) throw new Error("Luno root element is missing");

const projection = createBackendProjection();
const subscription = subscribeBackendProjection(bridge, projection).catch(() => () => {
  // Static mode has no backend event stream.
});
void Promise.all([bridge.getLibrary(), bridge.scanLibrary()]).catch(() => {
  // The static shell remains usable when the optional Tauri backend is absent.
});

createRoot(root).render(
  <StrictMode>
    <Shell player={<small>IPC contract v{CONTRACT_VERSION}</small>} backendProjection={projection} />
  </StrictMode>,
);

void subscription;
