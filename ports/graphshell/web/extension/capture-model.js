(() => {
  const DEFAULT_QUEUE_LIMIT = 2048;

  function defaultPolicy() {
    return {
      enabled: false,
      accepted_schemes: ["http", "https"],
      excluded_origins: [],
      strip_query: false,
      strip_fragment: true,
      dedupe_window_ms: 1000,
      segment_size: 64,
      retention_traces: 2048,
    };
  }

  function boundedInteger(value, fallback, minimum, maximum) {
    return Number.isInteger(value)
      ? Math.min(maximum, Math.max(minimum, value))
      : fallback;
  }

  function normalizePolicy(value) {
    const fallback = defaultPolicy();
    const acceptedSchemes = Array.isArray(value?.accepted_schemes)
      ? value.accepted_schemes
          .map((scheme) => String(scheme).trim().toLowerCase())
          .filter(Boolean)
      : fallback.accepted_schemes;
    const excludedOrigins = Array.isArray(value?.excluded_origins)
      ? value.excluded_origins
          .map((origin) => {
            try {
              return new URL(String(origin).trim()).origin;
            } catch {
              return null;
            }
          })
          .filter(Boolean)
      : fallback.excluded_origins;
    return {
      enabled: value?.enabled === true,
      accepted_schemes: [...new Set(acceptedSchemes)],
      excluded_origins: [...new Set(excludedOrigins)],
      strip_query: value?.strip_query === true,
      strip_fragment: value?.strip_fragment !== false,
      dedupe_window_ms: boundedInteger(
        value?.dedupe_window_ms,
        fallback.dedupe_window_ms,
        0,
        60_000,
      ),
      segment_size: boundedInteger(
        value?.segment_size,
        fallback.segment_size,
        1,
        1024,
      ),
      retention_traces: boundedInteger(
        value?.retention_traces,
        fallback.retention_traces,
        1,
        100_000,
      ),
    };
  }

  function sanitizeAddress(address, policy) {
    try {
      const url = new URL(String(address));
      const scheme = url.protocol.slice(0, -1).toLowerCase();
      if (!policy.accepted_schemes.includes(scheme)) {
        return null;
      }
      if (policy.excluded_origins.includes(url.origin)) {
        return null;
      }
      if (policy.strip_query) {
        url.search = "";
      }
      if (policy.strip_fragment) {
        url.hash = "";
      }
      return url.href;
    } catch {
      return null;
    }
  }

  function optionalText(value) {
    const text = typeof value === "string" ? value.trim() : "";
    return text || null;
  }

  function normalizeHistoryFilter(value) {
    const start = Number.isFinite(value?.start_ms)
      ? Math.max(0, Math.trunc(value.start_ms))
      : null;
    const end = Number.isFinite(value?.end_ms)
      ? Math.max(0, Math.trunc(value.end_ms))
      : null;
    return {
      start_ms: start,
      end_ms: end !== null && start !== null && end <= start ? null : end,
      persona: optionalText(value?.persona),
      device: optionalText(value?.device),
    };
  }

  function historyFilterFromControls(days, persona, device, nowMs = Date.now()) {
    const boundedDays = boundedInteger(days, 0, 0, 3650);
    const end = Math.max(0, Math.trunc(nowMs));
    return normalizeHistoryFilter({
      start_ms: boundedDays > 0 ? end - boundedDays * 86_400_000 : null,
      end_ms: null,
      persona,
      device,
    });
  }

  function forgetRequest(address, removeObject, configuredPolicy) {
    const policy = normalizePolicy(configuredPolicy);
    try {
      const url = new URL(String(address).trim());
      const scheme = url.protocol.slice(0, -1).toLowerCase();
      if (!policy.accepted_schemes.includes(scheme)) {
        return null;
      }
      if (policy.strip_query) {
        url.search = "";
      }
      if (policy.strip_fragment) {
        url.hash = "";
      }
      return {
        url: url.href,
        remove_object: removeObject === true,
      };
    } catch {
      return null;
    }
  }

  function sanitizeVisit(value, configuredPolicy) {
    const policy = normalizePolicy(configuredPolicy);
    if (!policy.enabled || value?.private === true) {
      return null;
    }
    const url = sanitizeAddress(value?.url, policy);
    if (!url) {
      return null;
    }
    const referrer = optionalText(value?.referrer_url);
    return {
      source: optionalText(value?.source)?.toLowerCase() ?? "browser",
      visit_id: optionalText(value?.visit_id),
      url,
      title: optionalText(value?.title),
      favicon_url: optionalText(value?.favicon_url),
      referrer_url: referrer ? sanitizeAddress(referrer, policy) : null,
      transition: optionalText(value?.transition)?.toLowerCase() ?? "unknown",
      at_ms: Number.isFinite(value?.at_ms) ? Math.max(0, Math.trunc(value.at_ms)) : 0,
      private: false,
    };
  }

  function queueKey(visit) {
    return visit.visit_id
      ? `${visit.source}\u0000${visit.visit_id}`
      : `${visit.source}\u0000${visit.url}\u0000${visit.at_ms}\u0000${visit.transition}`;
  }

  function mergeQueue(existing, incoming, limit = DEFAULT_QUEUE_LIMIT) {
    const merged = new Map();
    for (const visit of [...existing, ...incoming]) {
      merged.set(queueKey(visit), visit);
    }
    return [...merged.values()]
      .sort((left, right) => left.at_ms - right.at_ms)
      .slice(-boundedInteger(limit, DEFAULT_QUEUE_LIMIT, 1, 100_000));
  }

  globalThis.GraphshellCaptureModel = Object.freeze({
    DEFAULT_QUEUE_LIMIT,
    defaultPolicy,
    normalizePolicy,
    normalizeHistoryFilter,
    historyFilterFromControls,
    forgetRequest,
    sanitizeVisit,
    queueKey,
    mergeQueue,
  });
})();
