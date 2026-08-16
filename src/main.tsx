import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { Shell } from "./app/Shell";
import { bridge, isValidLibraryResponse } from "./app/bridge";
import { createBackendProjection, subscribeBackendProjection } from "./state/backend_projection";
import { Gate2Smoke } from "./app/gate2Smoke";
import { Gate3Smoke } from "./app/gate3Smoke";
import { Gate4Smoke } from "./app/gate4Smoke";
import { Gate5Smoke } from "./app/gate5Smoke";

const root = document.getElementById("root");
if (!root) throw new Error("Luno root element is missing");

const projection = createBackendProjection();
const viteEnv = (import.meta as ImportMeta & { env: { DEV: boolean; VITE_LUNO_GATE2_SMOKE?: string; VITE_LUNO_GATE2_SMOKE_ROOT?: string; VITE_LUNO_GATE2_SMOKE_TEMPORARY?: string; VITE_LUNO_GATE2_SMOKE_DESTINATION?: string; VITE_LUNO_GATE3_SMOKE?: string; VITE_LUNO_GATE3_SMOKE_ROOT?: string; VITE_LUNO_GATE3_SMOKE_TEMPORARY?: string; VITE_LUNO_GATE3_SMOKE_DESTINATION?: string; VITE_LUNO_GATE4_SMOKE?: string; VITE_LUNO_GATE4_SMOKE_HOME?: string; VITE_LUNO_GATE4_SMOKE_ROOT?: string; VITE_LUNO_GATE4_SMOKE_HELPER?: string; VITE_LUNO_GATE4_SMOKE_DESTINATION?: string; VITE_LUNO_GATE5_SMOKE?: string; VITE_LUNO_GATE5_SMOKE_ROOT?: string; VITE_LUNO_GATE5_SMOKE_PREVIEW?: string; VITE_LUNO_GATE5_SMOKE_VIDEO?: string } }).env;
const gate2SmokeConfig = viteEnv.DEV && viteEnv.VITE_LUNO_GATE2_SMOKE === "1"
  && viteEnv.VITE_LUNO_GATE2_SMOKE_ROOT
  && viteEnv.VITE_LUNO_GATE2_SMOKE_TEMPORARY
  && viteEnv.VITE_LUNO_GATE2_SMOKE_DESTINATION
  ? { root: viteEnv.VITE_LUNO_GATE2_SMOKE_ROOT, temporary: viteEnv.VITE_LUNO_GATE2_SMOKE_TEMPORARY, destination: viteEnv.VITE_LUNO_GATE2_SMOKE_DESTINATION }
  : null;
const gate3SmokeConfig = viteEnv.DEV && viteEnv.VITE_LUNO_GATE3_SMOKE === "1"
  && viteEnv.VITE_LUNO_GATE3_SMOKE_ROOT && viteEnv.VITE_LUNO_GATE3_SMOKE_TEMPORARY && viteEnv.VITE_LUNO_GATE3_SMOKE_DESTINATION
  ? { root: viteEnv.VITE_LUNO_GATE3_SMOKE_ROOT, temporary: viteEnv.VITE_LUNO_GATE3_SMOKE_TEMPORARY, destination: viteEnv.VITE_LUNO_GATE3_SMOKE_DESTINATION }
  : null;
const gate4SmokeConfig = viteEnv.DEV && viteEnv.VITE_LUNO_GATE4_SMOKE === "1"
  && viteEnv.VITE_LUNO_GATE4_SMOKE_HOME && viteEnv.VITE_LUNO_GATE4_SMOKE_ROOT && viteEnv.VITE_LUNO_GATE4_SMOKE_HELPER && viteEnv.VITE_LUNO_GATE4_SMOKE_DESTINATION
  ? { home: viteEnv.VITE_LUNO_GATE4_SMOKE_HOME, root: viteEnv.VITE_LUNO_GATE4_SMOKE_ROOT, helper: viteEnv.VITE_LUNO_GATE4_SMOKE_HELPER, destination: viteEnv.VITE_LUNO_GATE4_SMOKE_DESTINATION }
  : null;
const gate5SmokeConfig = viteEnv.DEV && viteEnv.VITE_LUNO_GATE5_SMOKE === "1"
  && viteEnv.VITE_LUNO_GATE5_SMOKE_ROOT && viteEnv.VITE_LUNO_GATE5_SMOKE_PREVIEW && viteEnv.VITE_LUNO_GATE5_SMOKE_VIDEO
  ? { root: viteEnv.VITE_LUNO_GATE5_SMOKE_ROOT, preview: viteEnv.VITE_LUNO_GATE5_SMOKE_PREVIEW, video: viteEnv.VITE_LUNO_GATE5_SMOKE_VIDEO }
  : null;

function App() {
  const [backendAvailable, setBackendAvailable] = useState(false);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void subscribeBackendProjection(bridge, projection)
      .then((cleanup) => {
        unlisten = cleanup;
        if (!active) {
          cleanup();
          return;
        }
        const library = bridge.getLibrary();
        // Scanning remains a startup action, but its failure does not mean IPC is down.
        void bridge.scanLibrary().catch(() => undefined);
        void library
          .then((response) => {
            if (!isValidLibraryResponse(response)) throw new Error("invalid library response");
            if (active) setBackendAvailable(true);
          })
          .catch(() => {
            if (active) setBackendAvailable(false);
          });
      })
      .catch(() => {
        if (active) setBackendAvailable(false);
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  return <><Shell backendProjection={projection} bridge={bridge} backendAvailable={backendAvailable} />{gate2SmokeConfig && <Gate2Smoke bridge={bridge} config={gate2SmokeConfig} />}{gate3SmokeConfig && <Gate3Smoke bridge={bridge} config={gate3SmokeConfig} />}{gate4SmokeConfig && <Gate4Smoke bridge={bridge} config={gate4SmokeConfig} />}{gate5SmokeConfig && <Gate5Smoke bridge={bridge} config={gate5SmokeConfig} />}</>;
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
