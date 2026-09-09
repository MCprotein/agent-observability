import { startPagedDashboard } from "./paged.js";

// This bundle is embedded only in the loopback shell, never in the self-contained export.
if (window.location.protocol === "http:" && window.location.hostname === "127.0.0.1") {
  startPagedDashboard({});
} else {
  const message = document.createElement("p");
  message.textContent = "Open this dashboard through agentobs dashboard on private localhost.";
  document.body.replaceChildren(message);
}
