const TOKEN_RATE_KEYS = [
  "input_tokens",
  "output_tokens",
  "cached_input_tokens",
  "cache_creation_input_tokens",
  "reasoning_output_tokens",
];

const TOKEN_METRIC_KEYS = {
  input_tokens: "input_tokens",
  output_tokens: "output_tokens",
  cached_input_tokens: "cached_input_tokens",
  cache_creation_input_tokens: "cache_creation_input_tokens",
  reasoning_output_tokens: "reasoning_output_tokens",
};

const TOKEN_SEMANTIC_KEYS = [
  "cached_input_tokens",
  "cache_creation_input_tokens",
  "reasoning_output_tokens",
];
const TOKEN_SEMANTICS = ["exclusive", "included_in_total"];

export function normalizeRateTable(rateTable) {
  if (!rateTable) {
    return null;
  }
  if (typeof rateTable !== "object" || Array.isArray(rateTable)) {
    throw new Error("rate table must be an object");
  }

  const models = rateTable.models;
  if (!models || typeof models !== "object" || Array.isArray(models)) {
    throw new Error("rate table models must be an object");
  }

  const normalizedModels = {};
  for (const [modelName, modelRates] of Object.entries(models)) {
    if (!modelRates || typeof modelRates !== "object" || Array.isArray(modelRates)) {
      throw new Error(`rate table model ${modelName} must be an object`);
    }

    normalizedModels[modelName] = {};
    for (const key of TOKEN_RATE_KEYS) {
      normalizedModels[modelName][key] = optionalRate(modelRates[key], `${modelName}.${key}`);
    }
    normalizedModels[modelName].token_semantics = normalizeTokenSemantics(
      modelRates.token_semantics,
      modelName,
    );
  }

  return {
    version: stringOrDefault(rateTable.version, "unversioned"),
    currency: stringOrDefault(rateTable.currency, "USD"),
    unit: normalizeRateUnit(rateTable.unit),
    assumption: stringOrDefault(
      rateTable.assumption,
      "Estimated from a local static rate table; not a billing statement.",
    ),
    models: normalizedModels,
  };
}

export function estimateSpanCost(record, rateTable) {
  const table = normalizeRateTable(rateTable);
  if (!table) {
    return unknownCost("missing_rate_table");
  }

  const model = record.agent?.model;
  if (!model || typeof model !== "string") {
    return costResult({
      status: "unknown",
      reason: "missing_model",
      rateTable: table,
      model,
    });
  }

  const modelRates = table.models[model];
  if (!modelRates) {
    return costResult({
      status: "unknown",
      reason: "missing_model_rate",
      rateTable: table,
      model,
    });
  }

  let amount = 0;
  const components = {};
  const missing = [];
  const { tokensByKey, semanticErrors } = billableTokenMetrics(record, modelRates);

  for (const key of TOKEN_RATE_KEYS) {
    const tokens = tokensByKey[key];
    if (tokens === undefined || tokens === 0) {
      continue;
    }

    const rate = modelRates[key];
    if (rate === undefined) {
      missing.push(key);
      continue;
    }

    const cost = (tokens / 1_000_000) * rate;
    components[key] = {
      tokens,
      rate_per_1m: rate,
      estimated_cost: roundCurrency(cost),
    };
    amount += cost;
  }

  if (Object.keys(components).length === 0 && missing.length === 0 && semanticErrors.length === 0) {
    return costResult({
      status: "unknown",
      reason: "missing_token_metrics",
      rateTable: table,
      model,
    });
  }

  return costResult({
    status: missing.length > 0 || semanticErrors.length > 0 ? "incomplete" : "estimated",
    reason:
      semanticErrors.length > 0
        ? "ambiguous_token_semantics"
        : missing.length > 0
          ? "missing_token_rates"
          : undefined,
    rateTable: table,
    model,
    amount: roundCurrency(amount),
    components,
    missing,
    semanticErrors,
  });
}

export function estimateCostForRecords(records, rateTable) {
  const table = normalizeRateTable(rateTable);
  if (!table) {
    return unknownCost("missing_rate_table");
  }

  const billableRecords = records.filter((record) => hasTokenMetrics(record));
  if (billableRecords.length === 0) {
    return costResult({
      status: "unknown",
      reason: "missing_token_metrics",
      rateTable: table,
    });
  }

  const spanCosts = billableRecords.map((record) => estimateSpanCost(record, table));
  const estimated = spanCosts.filter((cost) => cost.status === "estimated");
  const incomplete = spanCosts.filter((cost) => cost.status === "incomplete");
  const unknown = spanCosts.filter((cost) => cost.status === "unknown");
  const amount = spanCosts.reduce((sum, cost) => sum + (cost.estimated_cost ?? 0), 0);

  return {
    status: aggregateStatus(estimated, incomplete, unknown),
    estimated_cost: roundCurrency(amount),
    currency: table.currency,
    rate_table: {
      version: table.version,
      unit: table.unit,
    },
    cost: {
      assumption: table.assumption,
      incomplete_count: incomplete.length,
      unknown_count: unknown.length,
    },
  };
}

function aggregateStatus(estimated, incomplete, unknown) {
  if (estimated.length === 0 && incomplete.length === 0) {
    return "unknown";
  }
  if (incomplete.length > 0 || unknown.length > 0) {
    return "incomplete";
  }
  return "estimated";
}

function costResult({
  status,
  reason,
  rateTable,
  model,
  amount,
  components = {},
  missing = [],
  semanticErrors = [],
}) {
  return {
    status,
    ...(reason ? { reason } : {}),
    estimated_cost: amount,
    currency: rateTable.currency,
    model,
    rate_table: {
      version: rateTable.version,
      unit: rateTable.unit,
    },
    cost: {
      assumption: rateTable.assumption,
      ...(missing.length > 0 ? { missing } : {}),
      ...(semanticErrors.length > 0 ? { semantic_errors: semanticErrors } : {}),
      ...(Object.keys(components).length > 0 ? { components } : {}),
    },
  };
}

function normalizeTokenSemantics(value, modelName) {
  if (value === undefined) {
    return {};
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`rate table model ${modelName}.token_semantics must be an object`);
  }

  const normalized = {};
  for (const key of Object.keys(value)) {
    if (!TOKEN_SEMANTIC_KEYS.includes(key)) {
      throw new Error(`rate table model ${modelName}.token_semantics.${key} is not supported`);
    }
    if (!TOKEN_SEMANTICS.includes(value[key])) {
      throw new Error(
        `rate table model ${modelName}.token_semantics.${key} must be one of ${TOKEN_SEMANTICS.join(", ")}`,
      );
    }
    normalized[key] = value[key];
  }
  return normalized;
}

function billableTokenMetrics(record, modelRates) {
  const tokensByKey = Object.fromEntries(
    TOKEN_RATE_KEYS.map((key) => [key, metricNumber(record.metrics?.[TOKEN_METRIC_KEYS[key]])]),
  );
  const semanticErrors = [];

  applyIncludedBreakdowns({
    totalKey: "input_tokens",
    breakdownKeys: ["cached_input_tokens", "cache_creation_input_tokens"],
    tokensByKey,
    semantics: modelRates.token_semantics,
    semanticErrors,
  });
  applyIncludedBreakdowns({
    totalKey: "output_tokens",
    breakdownKeys: ["reasoning_output_tokens"],
    tokensByKey,
    semantics: modelRates.token_semantics,
    semanticErrors,
  });

  return { tokensByKey, semanticErrors };
}

function applyIncludedBreakdowns({ totalKey, breakdownKeys, tokensByKey, semantics, semanticErrors }) {
  const included = [];
  for (const key of breakdownKeys) {
    const tokens = tokensByKey[key];
    if (tokens === undefined || tokens === 0) {
      continue;
    }
    const semantic = semantics[key];
    if (!semantic) {
      semanticErrors.push(`${key}:missing_semantics`);
      tokensByKey[key] = undefined;
      continue;
    }
    if (semantic === "included_in_total") {
      included.push(key);
    }
  }

  if (included.length === 0) {
    return;
  }

  const total = tokensByKey[totalKey];
  const includedTotal = included.reduce((sum, key) => sum + tokensByKey[key], 0);
  if (total === undefined || includedTotal > total) {
    semanticErrors.push(`${totalKey}:invalid_included_breakdown`);
    for (const key of included) {
      tokensByKey[key] = undefined;
    }
    return;
  }
  tokensByKey[totalKey] = total - includedTotal;
}

function unknownCost(reason) {
  return {
    status: "unknown",
    reason,
    estimated_cost: undefined,
    currency: undefined,
    rate_table: {
      version: undefined,
      unit: undefined,
    },
    cost: {
      assumption: "No local rate table was supplied.",
    },
  };
}

function optionalRate(value, label) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`rate table ${label} must be a non-negative finite number`);
  }
  return value;
}

function normalizeRateUnit(value) {
  if (value === undefined) {
    return "per_1m_tokens";
  }
  if (value !== "per_1m_tokens") {
    throw new Error("rate table unit must be per_1m_tokens");
  }
  return value;
}

function metricNumber(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : undefined;
}

function hasTokenMetrics(record) {
  return TOKEN_RATE_KEYS.some((key) => metricNumber(record.metrics?.[TOKEN_METRIC_KEYS[key]]) !== undefined);
}

function roundCurrency(value) {
  return Math.round(value * 1_000_000_000) / 1_000_000_000;
}

function stringOrDefault(value, defaultValue) {
  return typeof value === "string" && value.length > 0 ? value : defaultValue;
}
