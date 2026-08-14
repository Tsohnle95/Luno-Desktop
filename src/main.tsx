import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { CONTRACT_VERSION } from "./shared/contracts";
import { Shell } from "./app/Shell";
import { bridge, isValidLibraryResponse } from "./app/bridge";
import { createBackendProjection, subscribeBackendProjection } from "./state/backend_projection";

const root = document.getElementById("root");
if (!root) throw new Error("Luno root element is missing");

const projection = createBackendProjection();

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

  return <Shell player={<small>IPC contract v{CONTRACT_VERSION}</small>} backendProjection={projection} bridge={bridge} backendAvailable={backendAvailable} />;
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
