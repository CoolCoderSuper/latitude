(() => {
  const shell = document.querySelector("[data-project-shell]");
  const dialog = shell?.querySelector("[data-share-dialog]");
  if (!shell || !dialog) return;

  const project = shell.dataset.project;
  const form = dialog.querySelector("[data-share-form]");
  const list = dialog.querySelector("[data-share-list]");
  const status = dialog.querySelector("[data-share-status]");
  const title = dialog.querySelector("[data-share-title]");
  const refreshButton = dialog.querySelector("[data-share-refresh]");
  const closeButton = dialog.querySelector("[data-share-close]");
  const apiUrl = "/__latitude/api/shares";
  let deployment = "";
  let shares = [];

  shell.querySelectorAll("[data-share-trigger]").forEach((button) => {
    button.addEventListener("click", () => openManager(button.dataset.deployment || ""));
  });
  closeButton.addEventListener("click", () => dialog.close());
  refreshButton.addEventListener("click", () => loadShares());
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const submitButton = form.querySelector("button[type='submit']");
    const data = new FormData(form);
    const password = String(data.get("password") || "").trim();
    const expirySeconds = Number(data.get("expiry") || 0);
    const payload = { project, deployment };
    if (password) payload.password = password;
    if (expirySeconds) {
      payload.expires_at = Math.floor(Date.now() / 1000) + expirySeconds;
    }

    submitButton.disabled = true;
    submitButton.textContent = "Creating…";
    clearStatus();
    try {
      const share = await requestJson(apiUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      shares.push(share);
      form.reset();
      showStatus("Share link created.", "success");
      renderShares();
    } catch (error) {
      showStatus(error.message, "error");
    } finally {
      submitButton.disabled = false;
      submitButton.textContent = "Create link";
    }
  });

  function openManager(nextDeployment) {
    deployment = nextDeployment;
    title.textContent = `Share ${deployment}`;
    form.reset();
    clearStatus();
    list.textContent = "Loading…";
    dialog.showModal();
    loadShares();
  }

  async function loadShares() {
    refreshButton.disabled = true;
    clearStatus();
    try {
      shares = await requestJson(apiUrl);
      renderShares();
    } catch (error) {
      list.textContent = "Share links could not be loaded.";
      showStatus(error.message, "error");
    } finally {
      refreshButton.disabled = false;
    }
  }

  function renderShares() {
    const matching = shares
      .filter((share) => share.project === project && share.deployment === deployment)
      .sort((left, right) => Number(left.expired) - Number(right.expired));
    list.replaceChildren();

    if (!matching.length) {
      const empty = document.createElement("div");
      empty.className = "share-empty";
      const icon = document.createElement("span");
      icon.className = "share-empty-icon";
      icon.textContent = "↗";
      const copy = document.createElement("div");
      const heading = document.createElement("strong");
      heading.textContent = "No links yet";
      const hint = document.createElement("span");
      hint.textContent = "Create a link above to share this deployment.";
      copy.append(heading, hint);
      empty.append(icon, copy);
      list.append(empty);
      return;
    }

    matching.forEach((share) => list.append(buildShareCard(share)));
  }

  function buildShareCard(share) {
    const card = document.createElement("article");
    card.className = "share-card";

    const details = document.createElement("div");
    details.className = "share-card-details";
    const token = document.createElement("strong");
    token.textContent = share.token;
    const meta = document.createElement("span");
    meta.className = share.expired ? "share-expired" : "";
    const expiry = share.expired
      ? "Expired"
      : share.expires_at
        ? `Expires ${new Date(share.expires_at * 1000).toLocaleString()}`
        : "Never expires";
    meta.textContent = `${expiry} · ${share.has_password ? "Password protected" : "Open link"}`;
    details.append(token, meta);

    const actions = document.createElement("div");
    actions.className = "share-card-actions";
    const sendButton = actionButton("Share", "share-send");
    sendButton.disabled = share.expired;
    sendButton.addEventListener("click", () => sendShare(share));
    const revokeButton = actionButton("Revoke", "share-revoke");
    revokeButton.addEventListener("click", () => revokeShare(share, revokeButton));
    actions.append(sendButton, revokeButton);
    card.append(details, actions);
    return card;
  }

  function actionButton(label, className) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = className;
    button.textContent = label;
    return button;
  }

  async function sendShare(share) {
    const url = new URL(share.href, window.location.origin).href;
    try {
      if (navigator.share) {
        await navigator.share({ title: `Share ${project}/${deployment}`, url });
        return;
      }
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(url);
        showStatus("Share link copied to the clipboard.", "success");
        return;
      }
      window.prompt("Copy this share link", url);
    } catch (error) {
      if (error?.name !== "AbortError") showStatus("The share link could not be shared.", "error");
    }
  }

  async function revokeShare(share, button) {
    if (!window.confirm("Revoke this share link? Anyone using it will lose access immediately.")) {
      return;
    }
    button.disabled = true;
    button.textContent = "Revoking…";
    clearStatus();
    try {
      await requestJson(`${apiUrl}/${encodeURIComponent(share.token)}`, { method: "DELETE" });
      shares = shares.filter((item) => item.token !== share.token);
      showStatus("Share link revoked.", "success");
      renderShares();
    } catch (error) {
      button.disabled = false;
      button.textContent = "Revoke";
      showStatus(error.message, "error");
    }
  }

  async function requestJson(url, options = {}) {
    const response = await fetch(url, {
      ...options,
      headers: { Accept: "application/json", ...(options.headers || {}) },
    });
    const payload = response.status === 204 ? null : await response.json().catch(() => null);
    if (!response.ok) {
      throw new Error(payload?.error || `Latitude returned ${response.status}.`);
    }
    return payload;
  }

  function showStatus(message, tone) {
    status.hidden = false;
    status.dataset.tone = tone;
    status.textContent = message;
  }

  function clearStatus() {
    status.hidden = true;
    status.textContent = "";
    delete status.dataset.tone;
  }
})();
