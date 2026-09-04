var __getOwnPropNames = Object.getOwnPropertyNames;
var __commonJS = (cb, mod) => function __require() {
  try {
    return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
  } catch (e) {
    throw mod = 0, e;
  }
};

// node_modules/ajv/dist/runtime/ucs2length.js
var require_ucs2length = __commonJS({
  "node_modules/ajv/dist/runtime/ucs2length.js"(exports) {
    "use strict";
    Object.defineProperty(exports, "__esModule", { value: true });
    function ucs2length(str) {
      const len = str.length;
      let length = 0;
      let pos = 0;
      let value;
      while (pos < len) {
        length++;
        value = str.charCodeAt(pos++);
        if (value >= 55296 && value <= 56319 && pos < len) {
          value = str.charCodeAt(pos);
          if ((value & 64512) === 56320)
            pos++;
        }
      }
      return length;
    }
    exports.default = ucs2length;
    ucs2length.code = 'require("ajv/dist/runtime/ucs2length").default';
  }
});

// validate-report-dto-v2.generated.js
var validate = validate20;
var validate_report_dto_v2_generated_default = validate20;
var schema32 = { "type": "object", "additionalProperties": false, "required": ["generatedSpans", "sessions", "turns", "llmRequests", "toolExecutions", "errors", "inputTokens", "outputTokens", "cachedInputTokens", "cacheCreationInputTokens", "reasoningOutputTokens", "latencyMs", "durationMs", "estimatedCost"], "properties": { "generatedSpans": { "type": "number" }, "sessions": { "type": "number" }, "turns": { "type": "number" }, "llmRequests": { "type": "number" }, "toolExecutions": { "type": "number" }, "errors": { "type": "number" }, "inputTokens": { "type": "number" }, "outputTokens": { "type": "number" }, "cachedInputTokens": { "type": "number" }, "cacheCreationInputTokens": { "type": "number" }, "reasoningOutputTokens": { "type": "number" }, "latencyMs": { "type": "number" }, "durationMs": { "type": "number" }, "estimatedCost": { "type": "number" } } };
var func1 = Object.prototype.hasOwnProperty;
var schema33 = { "type": "object", "additionalProperties": false, "required": ["status", "rate_table", "cost"], "properties": { "status": { "enum": ["estimated", "incomplete", "unknown"] }, "reason": { "type": "string" }, "estimated_cost": { "type": "number" }, "currency": { "type": "string" }, "model": { "type": "string" }, "rate_table": { "type": "object", "additionalProperties": false, "properties": { "version": { "type": "string" }, "unit": { "type": "string" } } }, "cost": { "$ref": "#/$defs/cost_detail" } } };
function validate22(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate22.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.assumption === void 0 && (missing0 = "assumption")) {
        validate22.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "assumption" || key0 === "incomplete_count" || key0 === "unknown_count" || key0 === "missing" || key0 === "semantic_errors" || key0 === "components")) {
            validate22.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.assumption !== void 0) {
            const _errs2 = errors;
            if (typeof data.assumption !== "string") {
              validate22.errors = [{ instancePath: instancePath + "/assumption", schemaPath: "#/properties/assumption/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.incomplete_count !== void 0) {
              let data1 = data.incomplete_count;
              const _errs4 = errors;
              if (!(typeof data1 == "number" && isFinite(data1))) {
                validate22.errors = [{ instancePath: instancePath + "/incomplete_count", schemaPath: "#/properties/incomplete_count/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                return false;
              }
              var valid0 = _errs4 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.unknown_count !== void 0) {
                let data2 = data.unknown_count;
                const _errs6 = errors;
                if (!(typeof data2 == "number" && isFinite(data2))) {
                  validate22.errors = [{ instancePath: instancePath + "/unknown_count", schemaPath: "#/properties/unknown_count/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                  return false;
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.missing !== void 0) {
                  let data3 = data.missing;
                  const _errs8 = errors;
                  const _errs9 = errors;
                  if (errors === _errs9) {
                    if (Array.isArray(data3)) {
                      var valid2 = true;
                      const len0 = data3.length;
                      for (let i0 = 0; i0 < len0; i0++) {
                        const _errs11 = errors;
                        if (typeof data3[i0] !== "string") {
                          validate22.errors = [{ instancePath: instancePath + "/missing/" + i0, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid2 = _errs11 === errors;
                        if (!valid2) {
                          break;
                        }
                      }
                    } else {
                      validate22.errors = [{ instancePath: instancePath + "/missing", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs8 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.semantic_errors !== void 0) {
                    let data5 = data.semantic_errors;
                    const _errs13 = errors;
                    const _errs14 = errors;
                    if (errors === _errs14) {
                      if (Array.isArray(data5)) {
                        var valid4 = true;
                        const len1 = data5.length;
                        for (let i1 = 0; i1 < len1; i1++) {
                          const _errs16 = errors;
                          if (typeof data5[i1] !== "string") {
                            validate22.errors = [{ instancePath: instancePath + "/semantic_errors/" + i1, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                          var valid4 = _errs16 === errors;
                          if (!valid4) {
                            break;
                          }
                        }
                      } else {
                        validate22.errors = [{ instancePath: instancePath + "/semantic_errors", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs13 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.components !== void 0) {
                      let data7 = data.components;
                      const _errs18 = errors;
                      if (errors === _errs18) {
                        if (data7 && typeof data7 == "object" && !Array.isArray(data7)) {
                          for (const key1 in data7) {
                            let data8 = data7[key1];
                            const _errs21 = errors;
                            const _errs22 = errors;
                            if (errors === _errs22) {
                              if (data8 && typeof data8 == "object" && !Array.isArray(data8)) {
                                let missing1;
                                if (data8.tokens === void 0 && (missing1 = "tokens") || data8.rate_per_1m === void 0 && (missing1 = "rate_per_1m") || data8.estimated_cost === void 0 && (missing1 = "estimated_cost")) {
                                  validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                                  return false;
                                } else {
                                  const _errs24 = errors;
                                  for (const key2 in data8) {
                                    if (!(key2 === "tokens" || key2 === "rate_per_1m" || key2 === "estimated_cost")) {
                                      validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                      return false;
                                      break;
                                    }
                                  }
                                  if (_errs24 === errors) {
                                    if (data8.tokens !== void 0) {
                                      let data9 = data8.tokens;
                                      const _errs25 = errors;
                                      if (!(typeof data9 == "number" && isFinite(data9))) {
                                        validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/tokens", schemaPath: "#/$defs/cost_component/properties/tokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                        return false;
                                      }
                                      var valid7 = _errs25 === errors;
                                    } else {
                                      var valid7 = true;
                                    }
                                    if (valid7) {
                                      if (data8.rate_per_1m !== void 0) {
                                        let data10 = data8.rate_per_1m;
                                        const _errs27 = errors;
                                        if (!(typeof data10 == "number" && isFinite(data10))) {
                                          validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/rate_per_1m", schemaPath: "#/$defs/cost_component/properties/rate_per_1m/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                          return false;
                                        }
                                        var valid7 = _errs27 === errors;
                                      } else {
                                        var valid7 = true;
                                      }
                                      if (valid7) {
                                        if (data8.estimated_cost !== void 0) {
                                          let data11 = data8.estimated_cost;
                                          const _errs29 = errors;
                                          if (!(typeof data11 == "number" && isFinite(data11))) {
                                            validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1") + "/estimated_cost", schemaPath: "#/$defs/cost_component/properties/estimated_cost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                            return false;
                                          }
                                          var valid7 = _errs29 === errors;
                                        } else {
                                          var valid7 = true;
                                        }
                                      }
                                    }
                                  }
                                }
                              } else {
                                validate22.errors = [{ instancePath: instancePath + "/components/" + key1.replace(/~/g, "~0").replace(/\//g, "~1"), schemaPath: "#/$defs/cost_component/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                return false;
                              }
                            }
                            var valid5 = _errs21 === errors;
                            if (!valid5) {
                              break;
                            }
                          }
                        } else {
                          validate22.errors = [{ instancePath: instancePath + "/components", schemaPath: "#/properties/components/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs18 === errors;
                    } else {
                      var valid0 = true;
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate22.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate22.errors = vErrors;
  return errors === 0;
}
validate22.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate21(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate21.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.status === void 0 && (missing0 = "status") || data.rate_table === void 0 && (missing0 = "rate_table") || data.cost === void 0 && (missing0 = "cost")) {
        validate21.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "status" || key0 === "reason" || key0 === "estimated_cost" || key0 === "currency" || key0 === "model" || key0 === "rate_table" || key0 === "cost")) {
            validate21.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.status !== void 0) {
            let data0 = data.status;
            const _errs2 = errors;
            if (!(data0 === "estimated" || data0 === "incomplete" || data0 === "unknown")) {
              validate21.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/enum", keyword: "enum", params: { allowedValues: schema33.properties.status.enum }, message: "must be equal to one of the allowed values" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.reason !== void 0) {
              const _errs3 = errors;
              if (typeof data.reason !== "string") {
                validate21.errors = [{ instancePath: instancePath + "/reason", schemaPath: "#/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.estimated_cost !== void 0) {
                let data2 = data.estimated_cost;
                const _errs5 = errors;
                if (!(typeof data2 == "number" && isFinite(data2))) {
                  validate21.errors = [{ instancePath: instancePath + "/estimated_cost", schemaPath: "#/properties/estimated_cost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                  return false;
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.currency !== void 0) {
                  const _errs7 = errors;
                  if (typeof data.currency !== "string") {
                    validate21.errors = [{ instancePath: instancePath + "/currency", schemaPath: "#/properties/currency/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.model !== void 0) {
                    const _errs9 = errors;
                    if (typeof data.model !== "string") {
                      validate21.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    var valid0 = _errs9 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.rate_table !== void 0) {
                      let data5 = data.rate_table;
                      const _errs11 = errors;
                      if (errors === _errs11) {
                        if (data5 && typeof data5 == "object" && !Array.isArray(data5)) {
                          const _errs13 = errors;
                          for (const key1 in data5) {
                            if (!(key1 === "version" || key1 === "unit")) {
                              validate21.errors = [{ instancePath: instancePath + "/rate_table", schemaPath: "#/properties/rate_table/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs13 === errors) {
                            if (data5.version !== void 0) {
                              const _errs14 = errors;
                              if (typeof data5.version !== "string") {
                                validate21.errors = [{ instancePath: instancePath + "/rate_table/version", schemaPath: "#/properties/rate_table/properties/version/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              var valid1 = _errs14 === errors;
                            } else {
                              var valid1 = true;
                            }
                            if (valid1) {
                              if (data5.unit !== void 0) {
                                const _errs16 = errors;
                                if (typeof data5.unit !== "string") {
                                  validate21.errors = [{ instancePath: instancePath + "/rate_table/unit", schemaPath: "#/properties/rate_table/properties/unit/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                var valid1 = _errs16 === errors;
                              } else {
                                var valid1 = true;
                              }
                            }
                          }
                        } else {
                          validate21.errors = [{ instancePath: instancePath + "/rate_table", schemaPath: "#/properties/rate_table/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs11 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.cost !== void 0) {
                        const _errs18 = errors;
                        if (!validate22(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                          vErrors = vErrors === null ? validate22.errors : vErrors.concat(validate22.errors);
                          errors = vErrors.length;
                        }
                        var valid0 = _errs18 === errors;
                      } else {
                        var valid0 = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate21.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate21.errors = vErrors;
  return errors === 0;
}
validate21.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate25(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate25.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repos === void 0 && (missing0 = "repos") || data.sessions === void 0 && (missing0 = "sessions") || data.turns === void 0 && (missing0 = "turns")) {
        validate25.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repos" || key0 === "sessions" || key0 === "turns" || key0 === "agents" || key0 === "models")) {
            validate25.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repos !== void 0) {
            let data0 = data.repos;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (Array.isArray(data0)) {
                var valid2 = true;
                const len0 = data0.length;
                for (let i0 = 0; i0 < len0; i0++) {
                  const _errs5 = errors;
                  if (typeof data0[i0] !== "string") {
                    validate25.errors = [{ instancePath: instancePath + "/repos/" + i0, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                  var valid2 = _errs5 === errors;
                  if (!valid2) {
                    break;
                  }
                }
              } else {
                validate25.errors = [{ instancePath: instancePath + "/repos", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.sessions !== void 0) {
              let data2 = data.sessions;
              const _errs7 = errors;
              const _errs8 = errors;
              if (errors === _errs8) {
                if (Array.isArray(data2)) {
                  var valid4 = true;
                  const len1 = data2.length;
                  for (let i1 = 0; i1 < len1; i1++) {
                    const _errs10 = errors;
                    if (typeof data2[i1] !== "string") {
                      validate25.errors = [{ instancePath: instancePath + "/sessions/" + i1, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    var valid4 = _errs10 === errors;
                    if (!valid4) {
                      break;
                    }
                  }
                } else {
                  validate25.errors = [{ instancePath: instancePath + "/sessions", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                  return false;
                }
              }
              var valid0 = _errs7 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.turns !== void 0) {
                let data4 = data.turns;
                const _errs12 = errors;
                const _errs13 = errors;
                if (errors === _errs13) {
                  if (Array.isArray(data4)) {
                    var valid6 = true;
                    const len2 = data4.length;
                    for (let i2 = 0; i2 < len2; i2++) {
                      const _errs15 = errors;
                      if (typeof data4[i2] !== "string") {
                        validate25.errors = [{ instancePath: instancePath + "/turns/" + i2, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                      var valid6 = _errs15 === errors;
                      if (!valid6) {
                        break;
                      }
                    }
                  } else {
                    validate25.errors = [{ instancePath: instancePath + "/turns", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                    return false;
                  }
                }
                var valid0 = _errs12 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.agents !== void 0) {
                  let data6 = data.agents;
                  const _errs17 = errors;
                  const _errs18 = errors;
                  if (errors === _errs18) {
                    if (Array.isArray(data6)) {
                      var valid8 = true;
                      const len3 = data6.length;
                      for (let i3 = 0; i3 < len3; i3++) {
                        const _errs20 = errors;
                        if (typeof data6[i3] !== "string") {
                          validate25.errors = [{ instancePath: instancePath + "/agents/" + i3, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid8 = _errs20 === errors;
                        if (!valid8) {
                          break;
                        }
                      }
                    } else {
                      validate25.errors = [{ instancePath: instancePath + "/agents", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                      return false;
                    }
                  }
                  var valid0 = _errs17 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.models !== void 0) {
                    let data8 = data.models;
                    const _errs22 = errors;
                    const _errs23 = errors;
                    if (errors === _errs23) {
                      if (Array.isArray(data8)) {
                        var valid10 = true;
                        const len4 = data8.length;
                        for (let i4 = 0; i4 < len4; i4++) {
                          const _errs25 = errors;
                          if (typeof data8[i4] !== "string") {
                            validate25.errors = [{ instancePath: instancePath + "/models/" + i4, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                          var valid10 = _errs25 === errors;
                          if (!valid10) {
                            break;
                          }
                        }
                      } else {
                        validate25.errors = [{ instancePath: instancePath + "/models", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                        return false;
                      }
                    }
                    var valid0 = _errs22 === errors;
                  } else {
                    var valid0 = true;
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate25.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate25.errors = vErrors;
  return errors === 0;
}
validate25.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema44 = { "type": "object", "additionalProperties": false, "required": ["traceId", "repo", "spans", "errors", "inputTokens", "outputTokens", "estimatedCost", "startTimeUnixMs", "endTimeUnixMs", "sessions", "turns"], "properties": { "traceId": { "type": "string" }, "repo": { "type": "string" }, "spans": { "type": "number" }, "errors": { "type": "number" }, "inputTokens": { "type": "number" }, "outputTokens": { "type": "number" }, "estimatedCost": { "type": "number" }, "startTimeUnixMs": { "type": "number" }, "endTimeUnixMs": { "type": ["number", "null"] }, "sessions": { "$ref": "#/$defs/strings" }, "turns": { "$ref": "#/$defs/strings" } } };
function validate27(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate27.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.traceId === void 0 && (missing0 = "traceId") || data.repo === void 0 && (missing0 = "repo") || data.spans === void 0 && (missing0 = "spans") || data.errors === void 0 && (missing0 = "errors") || data.inputTokens === void 0 && (missing0 = "inputTokens") || data.outputTokens === void 0 && (missing0 = "outputTokens") || data.estimatedCost === void 0 && (missing0 = "estimatedCost") || data.startTimeUnixMs === void 0 && (missing0 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing0 = "endTimeUnixMs") || data.sessions === void 0 && (missing0 = "sessions") || data.turns === void 0 && (missing0 = "turns")) {
        validate27.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func1.call(schema44.properties, key0)) {
            validate27.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.traceId !== void 0) {
            const _errs2 = errors;
            if (typeof data.traceId !== "string") {
              validate27.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/properties/traceId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.repo !== void 0) {
              const _errs4 = errors;
              if (typeof data.repo !== "string") {
                validate27.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid0 = _errs4 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.spans !== void 0) {
                let data2 = data.spans;
                const _errs6 = errors;
                if (!(typeof data2 == "number" && isFinite(data2))) {
                  validate27.errors = [{ instancePath: instancePath + "/spans", schemaPath: "#/properties/spans/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                  return false;
                }
                var valid0 = _errs6 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.errors !== void 0) {
                  let data3 = data.errors;
                  const _errs8 = errors;
                  if (!(typeof data3 == "number" && isFinite(data3))) {
                    validate27.errors = [{ instancePath: instancePath + "/errors", schemaPath: "#/properties/errors/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                    return false;
                  }
                  var valid0 = _errs8 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.inputTokens !== void 0) {
                    let data4 = data.inputTokens;
                    const _errs10 = errors;
                    if (!(typeof data4 == "number" && isFinite(data4))) {
                      validate27.errors = [{ instancePath: instancePath + "/inputTokens", schemaPath: "#/properties/inputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                      return false;
                    }
                    var valid0 = _errs10 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.outputTokens !== void 0) {
                      let data5 = data.outputTokens;
                      const _errs12 = errors;
                      if (!(typeof data5 == "number" && isFinite(data5))) {
                        validate27.errors = [{ instancePath: instancePath + "/outputTokens", schemaPath: "#/properties/outputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                        return false;
                      }
                      var valid0 = _errs12 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.estimatedCost !== void 0) {
                        let data6 = data.estimatedCost;
                        const _errs14 = errors;
                        if (!(typeof data6 == "number" && isFinite(data6))) {
                          validate27.errors = [{ instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                          return false;
                        }
                        var valid0 = _errs14 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.startTimeUnixMs !== void 0) {
                          let data7 = data.startTimeUnixMs;
                          const _errs16 = errors;
                          if (!(typeof data7 == "number" && isFinite(data7))) {
                            validate27.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                            return false;
                          }
                          var valid0 = _errs16 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.endTimeUnixMs !== void 0) {
                            let data8 = data.endTimeUnixMs;
                            const _errs18 = errors;
                            if (!(typeof data8 == "number" && isFinite(data8)) && data8 !== null) {
                              validate27.errors = [{ instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/type", keyword: "type", params: { type: schema44.properties.endTimeUnixMs.type }, message: "must be number,null" }];
                              return false;
                            }
                            var valid0 = _errs18 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.sessions !== void 0) {
                              let data9 = data.sessions;
                              const _errs20 = errors;
                              const _errs21 = errors;
                              if (errors === _errs21) {
                                if (Array.isArray(data9)) {
                                  var valid2 = true;
                                  const len0 = data9.length;
                                  for (let i0 = 0; i0 < len0; i0++) {
                                    const _errs23 = errors;
                                    if (typeof data9[i0] !== "string") {
                                      validate27.errors = [{ instancePath: instancePath + "/sessions/" + i0, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    var valid2 = _errs23 === errors;
                                    if (!valid2) {
                                      break;
                                    }
                                  }
                                } else {
                                  validate27.errors = [{ instancePath: instancePath + "/sessions", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                  return false;
                                }
                              }
                              var valid0 = _errs20 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.turns !== void 0) {
                                let data11 = data.turns;
                                const _errs25 = errors;
                                const _errs26 = errors;
                                if (errors === _errs26) {
                                  if (Array.isArray(data11)) {
                                    var valid4 = true;
                                    const len1 = data11.length;
                                    for (let i1 = 0; i1 < len1; i1++) {
                                      const _errs28 = errors;
                                      if (typeof data11[i1] !== "string") {
                                        validate27.errors = [{ instancePath: instancePath + "/turns/" + i1, schemaPath: "#/$defs/strings/items/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      var valid4 = _errs28 === errors;
                                      if (!valid4) {
                                        break;
                                      }
                                    }
                                  } else {
                                    validate27.errors = [{ instancePath: instancePath + "/turns", schemaPath: "#/$defs/strings/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                    return false;
                                  }
                                }
                                var valid0 = _errs25 === errors;
                              } else {
                                var valid0 = true;
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate27.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate27.errors = vErrors;
  return errors === 0;
}
validate27.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema47 = { "type": "object", "additionalProperties": false, "required": ["schemaVersion", "traceId", "spanId", "parentSpanId", "kind", "name", "status", "startTimeUnixMs", "endTimeUnixMs", "repo", "agent", "availability", "attributes", "metrics", "cost"], "properties": { "schemaVersion": { "type": "string" }, "traceId": { "type": "string" }, "spanId": { "type": "string" }, "parentSpanId": { "type": ["string", "null"] }, "kind": { "type": "string" }, "name": { "type": "string" }, "status": { "type": "string" }, "startTimeUnixMs": { "type": "number" }, "endTimeUnixMs": { "type": ["number", "null"] }, "repo": { "type": "string" }, "agent": { "$ref": "#/$defs/agent" }, "availability": { "$ref": "#/$defs/availability" }, "sessionId": { "type": "string" }, "turnId": { "type": "string" }, "toolName": { "type": "string" }, "attributes": { "$ref": "#/$defs/attributes" }, "metrics": { "$ref": "#/$defs/metrics" }, "estimatedCost": { "type": "number" }, "cost": { "$ref": "#/$defs/cost" } }, "allOf": [{ "if": { "properties": { "metrics": { "type": "object", "anyOf": [{ "type": "object", "properties": { "inputTokens": true }, "required": ["inputTokens"] }, { "type": "object", "properties": { "outputTokens": true }, "required": ["outputTokens"] }, { "type": "object", "properties": { "totalTokens": true }, "required": ["totalTokens"] }, { "type": "object", "properties": { "totalInputTokens": true }, "required": ["totalInputTokens"] }, { "type": "object", "properties": { "totalOutputTokens": true }, "required": ["totalOutputTokens"] }, { "type": "object", "properties": { "totalAccumulatedTokens": true }, "required": ["totalAccumulatedTokens"] }] } } }, "then": { "if": { "properties": { "metrics": { "type": "object", "anyOf": [{ "type": "object", "properties": { "totalTokens": true }, "required": ["totalTokens"] }, { "type": "object", "properties": { "totalAccumulatedTokens": true }, "required": ["totalAccumulatedTokens"] }, { "type": "object", "properties": { "inputTokens": true, "outputTokens": true }, "required": ["inputTokens", "outputTokens"] }, { "type": "object", "properties": { "totalInputTokens": true, "totalOutputTokens": true }, "required": ["totalInputTokens", "totalOutputTokens"] }] } } }, "then": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "const": "available" } } } } } } }, "else": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "const": "source_unavailable" }, "reason": { "const": "partial_token_metrics" } } } } } } } }, "else": { "properties": { "availability": { "type": "object", "properties": { "tokens": { "type": "object", "properties": { "state": { "not": { "const": "available" } } } } } } } } }] };
var schema71 = { "type": "object", "additionalProperties": false, "properties": { "inputTokens": { "type": "number" }, "outputTokens": { "type": "number" }, "cachedInputTokens": { "type": "number" }, "cacheCreationInputTokens": { "type": "number" }, "reasoningOutputTokens": { "type": "number" }, "totalTokens": { "type": "number" }, "latencyMs": { "type": "number" }, "durationMs": { "type": "number" }, "totalInputTokens": { "type": "number" }, "totalOutputTokens": { "type": "number" }, "totalCachedInputTokens": { "type": "number" }, "totalReasoningOutputTokens": { "type": "number" }, "totalAccumulatedTokens": { "type": "number" }, "contextWindowTokens": { "type": "number" } } };
var schema50 = { "type": "object", "additionalProperties": false, "required": ["state", "reason"], "properties": { "state": { "enum": ["available", "source_unavailable", "withheld", "not_applicable", "private_lookup"] }, "reason": { "type": "string", "minLength": 1, "maxLength": 96 } } };
var func4 = require_ucs2length().default;
function validate30(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate30.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.repository === void 0 && (missing0 = "repository") || data.turn === void 0 && (missing0 = "turn") || data.model === void 0 && (missing0 = "model") || data.tokens === void 0 && (missing0 = "tokens") || data.latency === void 0 && (missing0 = "latency") || data.sourceLocation === void 0 && (missing0 = "sourceLocation") || data.requestContent === void 0 && (missing0 = "requestContent") || data.responseContent === void 0 && (missing0 = "responseContent")) {
        validate30.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "repository" || key0 === "turn" || key0 === "model" || key0 === "tokens" || key0 === "latency" || key0 === "sourceLocation" || key0 === "requestContent" || key0 === "responseContent")) {
            validate30.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.repository !== void 0) {
            let data0 = data.repository;
            const _errs2 = errors;
            const _errs3 = errors;
            if (errors === _errs3) {
              if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                let missing1;
                if (data0.state === void 0 && (missing1 = "state") || data0.reason === void 0 && (missing1 = "reason")) {
                  validate30.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                  return false;
                } else {
                  const _errs5 = errors;
                  for (const key1 in data0) {
                    if (!(key1 === "state" || key1 === "reason")) {
                      validate30.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                      return false;
                      break;
                    }
                  }
                  if (_errs5 === errors) {
                    if (data0.state !== void 0) {
                      let data1 = data0.state;
                      const _errs6 = errors;
                      if (!(data1 === "available" || data1 === "source_unavailable" || data1 === "withheld" || data1 === "not_applicable" || data1 === "private_lookup")) {
                        validate30.errors = [{ instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid2 = _errs6 === errors;
                    } else {
                      var valid2 = true;
                    }
                    if (valid2) {
                      if (data0.reason !== void 0) {
                        let data2 = data0.reason;
                        const _errs7 = errors;
                        if (errors === _errs7) {
                          if (typeof data2 === "string") {
                            if (func4(data2) > 96) {
                              validate30.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                              return false;
                            } else {
                              if (func4(data2) < 1) {
                                validate30.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                return false;
                              }
                            }
                          } else {
                            validate30.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                        }
                        var valid2 = _errs7 === errors;
                      } else {
                        var valid2 = true;
                      }
                    }
                  }
                }
              } else {
                validate30.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                return false;
              }
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.turn !== void 0) {
              let data3 = data.turn;
              const _errs9 = errors;
              const _errs10 = errors;
              if (errors === _errs10) {
                if (data3 && typeof data3 == "object" && !Array.isArray(data3)) {
                  let missing2;
                  if (data3.state === void 0 && (missing2 = "state") || data3.reason === void 0 && (missing2 = "reason")) {
                    validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                    return false;
                  } else {
                    const _errs12 = errors;
                    for (const key2 in data3) {
                      if (!(key2 === "state" || key2 === "reason")) {
                        validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                        return false;
                        break;
                      }
                    }
                    if (_errs12 === errors) {
                      if (data3.state !== void 0) {
                        let data4 = data3.state;
                        const _errs13 = errors;
                        if (!(data4 === "available" || data4 === "source_unavailable" || data4 === "withheld" || data4 === "not_applicable" || data4 === "private_lookup")) {
                          validate30.errors = [{ instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid4 = _errs13 === errors;
                      } else {
                        var valid4 = true;
                      }
                      if (valid4) {
                        if (data3.reason !== void 0) {
                          let data5 = data3.reason;
                          const _errs14 = errors;
                          if (errors === _errs14) {
                            if (typeof data5 === "string") {
                              if (func4(data5) > 96) {
                                validate30.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                return false;
                              } else {
                                if (func4(data5) < 1) {
                                  validate30.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                  return false;
                                }
                              }
                            } else {
                              validate30.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                          }
                          var valid4 = _errs14 === errors;
                        } else {
                          var valid4 = true;
                        }
                      }
                    }
                  }
                } else {
                  validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
              var valid0 = _errs9 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.model !== void 0) {
                let data6 = data.model;
                const _errs16 = errors;
                const _errs17 = errors;
                if (errors === _errs17) {
                  if (data6 && typeof data6 == "object" && !Array.isArray(data6)) {
                    let missing3;
                    if (data6.state === void 0 && (missing3 = "state") || data6.reason === void 0 && (missing3 = "reason")) {
                      validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" }];
                      return false;
                    } else {
                      const _errs19 = errors;
                      for (const key3 in data6) {
                        if (!(key3 === "state" || key3 === "reason")) {
                          validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" }];
                          return false;
                          break;
                        }
                      }
                      if (_errs19 === errors) {
                        if (data6.state !== void 0) {
                          let data7 = data6.state;
                          const _errs20 = errors;
                          if (!(data7 === "available" || data7 === "source_unavailable" || data7 === "withheld" || data7 === "not_applicable" || data7 === "private_lookup")) {
                            validate30.errors = [{ instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid6 = _errs20 === errors;
                        } else {
                          var valid6 = true;
                        }
                        if (valid6) {
                          if (data6.reason !== void 0) {
                            let data8 = data6.reason;
                            const _errs21 = errors;
                            if (errors === _errs21) {
                              if (typeof data8 === "string") {
                                if (func4(data8) > 96) {
                                  validate30.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                  return false;
                                } else {
                                  if (func4(data8) < 1) {
                                    validate30.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                    return false;
                                  }
                                }
                              } else {
                                validate30.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                            }
                            var valid6 = _errs21 === errors;
                          } else {
                            var valid6 = true;
                          }
                        }
                      }
                    }
                  } else {
                    validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
                var valid0 = _errs16 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.tokens !== void 0) {
                  let data9 = data.tokens;
                  const _errs23 = errors;
                  const _errs24 = errors;
                  if (errors === _errs24) {
                    if (data9 && typeof data9 == "object" && !Array.isArray(data9)) {
                      let missing4;
                      if (data9.state === void 0 && (missing4 = "state") || data9.reason === void 0 && (missing4 = "reason")) {
                        validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
                        return false;
                      } else {
                        const _errs26 = errors;
                        for (const key4 in data9) {
                          if (!(key4 === "state" || key4 === "reason")) {
                            validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" }];
                            return false;
                            break;
                          }
                        }
                        if (_errs26 === errors) {
                          if (data9.state !== void 0) {
                            let data10 = data9.state;
                            const _errs27 = errors;
                            if (!(data10 === "available" || data10 === "source_unavailable" || data10 === "withheld" || data10 === "not_applicable" || data10 === "private_lookup")) {
                              validate30.errors = [{ instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid8 = _errs27 === errors;
                          } else {
                            var valid8 = true;
                          }
                          if (valid8) {
                            if (data9.reason !== void 0) {
                              let data11 = data9.reason;
                              const _errs28 = errors;
                              if (errors === _errs28) {
                                if (typeof data11 === "string") {
                                  if (func4(data11) > 96) {
                                    validate30.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                    return false;
                                  } else {
                                    if (func4(data11) < 1) {
                                      validate30.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                      return false;
                                    }
                                  }
                                } else {
                                  validate30.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                              }
                              var valid8 = _errs28 === errors;
                            } else {
                              var valid8 = true;
                            }
                          }
                        }
                      }
                    } else {
                      validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                      return false;
                    }
                  }
                  var valid0 = _errs23 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.latency !== void 0) {
                    let data12 = data.latency;
                    const _errs30 = errors;
                    const _errs31 = errors;
                    if (errors === _errs31) {
                      if (data12 && typeof data12 == "object" && !Array.isArray(data12)) {
                        let missing5;
                        if (data12.state === void 0 && (missing5 = "state") || data12.reason === void 0 && (missing5 = "reason")) {
                          validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing5 }, message: "must have required property '" + missing5 + "'" }];
                          return false;
                        } else {
                          const _errs33 = errors;
                          for (const key5 in data12) {
                            if (!(key5 === "state" || key5 === "reason")) {
                              validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key5 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs33 === errors) {
                            if (data12.state !== void 0) {
                              let data13 = data12.state;
                              const _errs34 = errors;
                              if (!(data13 === "available" || data13 === "source_unavailable" || data13 === "withheld" || data13 === "not_applicable" || data13 === "private_lookup")) {
                                validate30.errors = [{ instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                return false;
                              }
                              var valid10 = _errs34 === errors;
                            } else {
                              var valid10 = true;
                            }
                            if (valid10) {
                              if (data12.reason !== void 0) {
                                let data14 = data12.reason;
                                const _errs35 = errors;
                                if (errors === _errs35) {
                                  if (typeof data14 === "string") {
                                    if (func4(data14) > 96) {
                                      validate30.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                      return false;
                                    } else {
                                      if (func4(data14) < 1) {
                                        validate30.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                        return false;
                                      }
                                    }
                                  } else {
                                    validate30.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                    return false;
                                  }
                                }
                                var valid10 = _errs35 === errors;
                              } else {
                                var valid10 = true;
                              }
                            }
                          }
                        }
                      } else {
                        validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                        return false;
                      }
                    }
                    var valid0 = _errs30 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceLocation !== void 0) {
                      let data15 = data.sourceLocation;
                      const _errs37 = errors;
                      const _errs38 = errors;
                      if (errors === _errs38) {
                        if (data15 && typeof data15 == "object" && !Array.isArray(data15)) {
                          let missing6;
                          if (data15.state === void 0 && (missing6 = "state") || data15.reason === void 0 && (missing6 = "reason")) {
                            validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing6 }, message: "must have required property '" + missing6 + "'" }];
                            return false;
                          } else {
                            const _errs40 = errors;
                            for (const key6 in data15) {
                              if (!(key6 === "state" || key6 === "reason")) {
                                validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key6 }, message: "must NOT have additional properties" }];
                                return false;
                                break;
                              }
                            }
                            if (_errs40 === errors) {
                              if (data15.state !== void 0) {
                                let data16 = data15.state;
                                const _errs41 = errors;
                                if (!(data16 === "available" || data16 === "source_unavailable" || data16 === "withheld" || data16 === "not_applicable" || data16 === "private_lookup")) {
                                  validate30.errors = [{ instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid12 = _errs41 === errors;
                              } else {
                                var valid12 = true;
                              }
                              if (valid12) {
                                if (data15.reason !== void 0) {
                                  let data17 = data15.reason;
                                  const _errs42 = errors;
                                  if (errors === _errs42) {
                                    if (typeof data17 === "string") {
                                      if (func4(data17) > 96) {
                                        validate30.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                        return false;
                                      } else {
                                        if (func4(data17) < 1) {
                                          validate30.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                          return false;
                                        }
                                      }
                                    } else {
                                      validate30.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                  }
                                  var valid12 = _errs42 === errors;
                                } else {
                                  var valid12 = true;
                                }
                              }
                            }
                          }
                        } else {
                          validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs37 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.requestContent !== void 0) {
                        let data18 = data.requestContent;
                        const _errs44 = errors;
                        const _errs45 = errors;
                        if (errors === _errs45) {
                          if (data18 && typeof data18 == "object" && !Array.isArray(data18)) {
                            let missing7;
                            if (data18.state === void 0 && (missing7 = "state") || data18.reason === void 0 && (missing7 = "reason")) {
                              validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing7 }, message: "must have required property '" + missing7 + "'" }];
                              return false;
                            } else {
                              const _errs47 = errors;
                              for (const key7 in data18) {
                                if (!(key7 === "state" || key7 === "reason")) {
                                  validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key7 }, message: "must NOT have additional properties" }];
                                  return false;
                                  break;
                                }
                              }
                              if (_errs47 === errors) {
                                if (data18.state !== void 0) {
                                  let data19 = data18.state;
                                  const _errs48 = errors;
                                  if (!(data19 === "available" || data19 === "source_unavailable" || data19 === "withheld" || data19 === "not_applicable" || data19 === "private_lookup")) {
                                    validate30.errors = [{ instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                    return false;
                                  }
                                  var valid14 = _errs48 === errors;
                                } else {
                                  var valid14 = true;
                                }
                                if (valid14) {
                                  if (data18.reason !== void 0) {
                                    let data20 = data18.reason;
                                    const _errs49 = errors;
                                    if (errors === _errs49) {
                                      if (typeof data20 === "string") {
                                        if (func4(data20) > 96) {
                                          validate30.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                          return false;
                                        } else {
                                          if (func4(data20) < 1) {
                                            validate30.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                            return false;
                                          }
                                        }
                                      } else {
                                        validate30.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                    }
                                    var valid14 = _errs49 === errors;
                                  } else {
                                    var valid14 = true;
                                  }
                                }
                              }
                            }
                          } else {
                            validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                            return false;
                          }
                        }
                        var valid0 = _errs44 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.responseContent !== void 0) {
                          let data21 = data.responseContent;
                          const _errs51 = errors;
                          const _errs52 = errors;
                          if (errors === _errs52) {
                            if (data21 && typeof data21 == "object" && !Array.isArray(data21)) {
                              let missing8;
                              if (data21.state === void 0 && (missing8 = "state") || data21.reason === void 0 && (missing8 = "reason")) {
                                validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing8 }, message: "must have required property '" + missing8 + "'" }];
                                return false;
                              } else {
                                const _errs54 = errors;
                                for (const key8 in data21) {
                                  if (!(key8 === "state" || key8 === "reason")) {
                                    validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key8 }, message: "must NOT have additional properties" }];
                                    return false;
                                    break;
                                  }
                                }
                                if (_errs54 === errors) {
                                  if (data21.state !== void 0) {
                                    let data22 = data21.state;
                                    const _errs55 = errors;
                                    if (!(data22 === "available" || data22 === "source_unavailable" || data22 === "withheld" || data22 === "not_applicable" || data22 === "private_lookup")) {
                                      validate30.errors = [{ instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid16 = _errs55 === errors;
                                  } else {
                                    var valid16 = true;
                                  }
                                  if (valid16) {
                                    if (data21.reason !== void 0) {
                                      let data23 = data21.reason;
                                      const _errs56 = errors;
                                      if (errors === _errs56) {
                                        if (typeof data23 === "string") {
                                          if (func4(data23) > 96) {
                                            validate30.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/maxLength", keyword: "maxLength", params: { limit: 96 }, message: "must NOT have more than 96 characters" }];
                                            return false;
                                          } else {
                                            if (func4(data23) < 1) {
                                              validate30.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/minLength", keyword: "minLength", params: { limit: 1 }, message: "must NOT have fewer than 1 characters" }];
                                              return false;
                                            }
                                          }
                                        } else {
                                          validate30.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                      }
                                      var valid16 = _errs56 === errors;
                                    } else {
                                      var valid16 = true;
                                    }
                                  }
                                }
                              }
                            } else {
                              validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                              return false;
                            }
                          }
                          var valid0 = _errs51 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate30.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate30.errors = vErrors;
  return errors === 0;
}
validate30.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
var schema58 = { "type": "object", "additionalProperties": false, "properties": { "source": { "$ref": "#/$defs/scalar" }, "event_type": { "$ref": "#/$defs/scalar" }, "envelope_type": { "$ref": "#/$defs/scalar" }, "session_id": { "$ref": "#/$defs/scalar" }, "turn_id": { "$ref": "#/$defs/scalar" }, "request_id": { "$ref": "#/$defs/scalar" }, "call_id": { "$ref": "#/$defs/scalar" }, "tool_name": { "$ref": "#/$defs/scalar" }, "phase": { "$ref": "#/$defs/scalar" }, "exit_code": { "$ref": "#/$defs/scalar" }, "sandbox": { "$ref": "#/$defs/scalar" }, "approval": { "$ref": "#/$defs/scalar" } } };
var schema59 = { "type": ["string", "number", "boolean"] };
function validate32(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate32.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      const _errs1 = errors;
      for (const key0 in data) {
        if (!func1.call(schema58.properties, key0)) {
          validate32.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
          return false;
          break;
        }
      }
      if (_errs1 === errors) {
        if (data.source !== void 0) {
          let data0 = data.source;
          const _errs2 = errors;
          if (typeof data0 !== "string" && !(typeof data0 == "number" && isFinite(data0)) && typeof data0 !== "boolean") {
            validate32.errors = [{ instancePath: instancePath + "/source", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
            return false;
          }
          var valid0 = _errs2 === errors;
        } else {
          var valid0 = true;
        }
        if (valid0) {
          if (data.event_type !== void 0) {
            let data1 = data.event_type;
            const _errs5 = errors;
            if (typeof data1 !== "string" && !(typeof data1 == "number" && isFinite(data1)) && typeof data1 !== "boolean") {
              validate32.errors = [{ instancePath: instancePath + "/event_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
              return false;
            }
            var valid0 = _errs5 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.envelope_type !== void 0) {
              let data2 = data.envelope_type;
              const _errs8 = errors;
              if (typeof data2 !== "string" && !(typeof data2 == "number" && isFinite(data2)) && typeof data2 !== "boolean") {
                validate32.errors = [{ instancePath: instancePath + "/envelope_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                return false;
              }
              var valid0 = _errs8 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.session_id !== void 0) {
                let data3 = data.session_id;
                const _errs11 = errors;
                if (typeof data3 !== "string" && !(typeof data3 == "number" && isFinite(data3)) && typeof data3 !== "boolean") {
                  validate32.errors = [{ instancePath: instancePath + "/session_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                  return false;
                }
                var valid0 = _errs11 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.turn_id !== void 0) {
                  let data4 = data.turn_id;
                  const _errs14 = errors;
                  if (typeof data4 !== "string" && !(typeof data4 == "number" && isFinite(data4)) && typeof data4 !== "boolean") {
                    validate32.errors = [{ instancePath: instancePath + "/turn_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                    return false;
                  }
                  var valid0 = _errs14 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.request_id !== void 0) {
                    let data5 = data.request_id;
                    const _errs17 = errors;
                    if (typeof data5 !== "string" && !(typeof data5 == "number" && isFinite(data5)) && typeof data5 !== "boolean") {
                      validate32.errors = [{ instancePath: instancePath + "/request_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                      return false;
                    }
                    var valid0 = _errs17 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.call_id !== void 0) {
                      let data6 = data.call_id;
                      const _errs20 = errors;
                      if (typeof data6 !== "string" && !(typeof data6 == "number" && isFinite(data6)) && typeof data6 !== "boolean") {
                        validate32.errors = [{ instancePath: instancePath + "/call_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                        return false;
                      }
                      var valid0 = _errs20 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.tool_name !== void 0) {
                        let data7 = data.tool_name;
                        const _errs23 = errors;
                        if (typeof data7 !== "string" && !(typeof data7 == "number" && isFinite(data7)) && typeof data7 !== "boolean") {
                          validate32.errors = [{ instancePath: instancePath + "/tool_name", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                          return false;
                        }
                        var valid0 = _errs23 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.phase !== void 0) {
                          let data8 = data.phase;
                          const _errs26 = errors;
                          if (typeof data8 !== "string" && !(typeof data8 == "number" && isFinite(data8)) && typeof data8 !== "boolean") {
                            validate32.errors = [{ instancePath: instancePath + "/phase", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                            return false;
                          }
                          var valid0 = _errs26 === errors;
                        } else {
                          var valid0 = true;
                        }
                        if (valid0) {
                          if (data.exit_code !== void 0) {
                            let data9 = data.exit_code;
                            const _errs29 = errors;
                            if (typeof data9 !== "string" && !(typeof data9 == "number" && isFinite(data9)) && typeof data9 !== "boolean") {
                              validate32.errors = [{ instancePath: instancePath + "/exit_code", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                              return false;
                            }
                            var valid0 = _errs29 === errors;
                          } else {
                            var valid0 = true;
                          }
                          if (valid0) {
                            if (data.sandbox !== void 0) {
                              let data10 = data.sandbox;
                              const _errs32 = errors;
                              if (typeof data10 !== "string" && !(typeof data10 == "number" && isFinite(data10)) && typeof data10 !== "boolean") {
                                validate32.errors = [{ instancePath: instancePath + "/sandbox", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                                return false;
                              }
                              var valid0 = _errs32 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.approval !== void 0) {
                                let data11 = data.approval;
                                const _errs35 = errors;
                                if (typeof data11 !== "string" && !(typeof data11 == "number" && isFinite(data11)) && typeof data11 !== "boolean") {
                                  validate32.errors = [{ instancePath: instancePath + "/approval", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema59.type }, message: "must be string,number,boolean" }];
                                  return false;
                                }
                                var valid0 = _errs35 === errors;
                              } else {
                                var valid0 = true;
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate32.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate32.errors = vErrors;
  return errors === 0;
}
validate32.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate29(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate29.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  const _errs2 = errors;
  let valid1 = true;
  const _errs3 = errors;
  if (data && typeof data == "object" && !Array.isArray(data)) {
    if (data.metrics !== void 0) {
      let data0 = data.metrics;
      if (!(data0 && typeof data0 == "object" && !Array.isArray(data0))) {
        const err0 = {};
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      }
      const _errs6 = errors;
      let valid3 = false;
      const _errs7 = errors;
      if (errors === _errs7) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing0;
          if (data0.inputTokens === void 0 && (missing0 = "inputTokens")) {
            const err1 = {};
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
          }
        } else {
          const err2 = {};
          if (vErrors === null) {
            vErrors = [err2];
          } else {
            vErrors.push(err2);
          }
          errors++;
        }
      }
      var _valid1 = _errs7 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        var props0 = {};
        props0.inputTokens = true;
      }
      const _errs9 = errors;
      if (errors === _errs9) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing1;
          if (data0.outputTokens === void 0 && (missing1 = "outputTokens")) {
            const err3 = {};
            if (vErrors === null) {
              vErrors = [err3];
            } else {
              vErrors.push(err3);
            }
            errors++;
          }
        } else {
          const err4 = {};
          if (vErrors === null) {
            vErrors = [err4];
          } else {
            vErrors.push(err4);
          }
          errors++;
        }
      }
      var _valid1 = _errs9 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.outputTokens = true;
        }
      }
      const _errs11 = errors;
      if (errors === _errs11) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing2;
          if (data0.totalTokens === void 0 && (missing2 = "totalTokens")) {
            const err5 = {};
            if (vErrors === null) {
              vErrors = [err5];
            } else {
              vErrors.push(err5);
            }
            errors++;
          }
        } else {
          const err6 = {};
          if (vErrors === null) {
            vErrors = [err6];
          } else {
            vErrors.push(err6);
          }
          errors++;
        }
      }
      var _valid1 = _errs11 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalTokens = true;
        }
      }
      const _errs13 = errors;
      if (errors === _errs13) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing3;
          if (data0.totalInputTokens === void 0 && (missing3 = "totalInputTokens")) {
            const err7 = {};
            if (vErrors === null) {
              vErrors = [err7];
            } else {
              vErrors.push(err7);
            }
            errors++;
          }
        } else {
          const err8 = {};
          if (vErrors === null) {
            vErrors = [err8];
          } else {
            vErrors.push(err8);
          }
          errors++;
        }
      }
      var _valid1 = _errs13 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalInputTokens = true;
        }
      }
      const _errs15 = errors;
      if (errors === _errs15) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing4;
          if (data0.totalOutputTokens === void 0 && (missing4 = "totalOutputTokens")) {
            const err9 = {};
            if (vErrors === null) {
              vErrors = [err9];
            } else {
              vErrors.push(err9);
            }
            errors++;
          }
        } else {
          const err10 = {};
          if (vErrors === null) {
            vErrors = [err10];
          } else {
            vErrors.push(err10);
          }
          errors++;
        }
      }
      var _valid1 = _errs15 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalOutputTokens = true;
        }
      }
      const _errs17 = errors;
      if (errors === _errs17) {
        if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
          let missing5;
          if (data0.totalAccumulatedTokens === void 0 && (missing5 = "totalAccumulatedTokens")) {
            const err11 = {};
            if (vErrors === null) {
              vErrors = [err11];
            } else {
              vErrors.push(err11);
            }
            errors++;
          }
        } else {
          const err12 = {};
          if (vErrors === null) {
            vErrors = [err12];
          } else {
            vErrors.push(err12);
          }
          errors++;
        }
      }
      var _valid1 = _errs17 === errors;
      valid3 = valid3 || _valid1;
      if (_valid1) {
        if (props0 !== true) {
          props0 = props0 || {};
          props0.totalAccumulatedTokens = true;
        }
      }
      if (!valid3) {
        const err13 = {};
        if (vErrors === null) {
          vErrors = [err13];
        } else {
          vErrors.push(err13);
        }
        errors++;
      } else {
        errors = _errs6;
        if (vErrors !== null) {
          if (_errs6) {
            vErrors.length = _errs6;
          } else {
            vErrors = null;
          }
        }
      }
    }
  }
  var _valid0 = _errs3 === errors;
  errors = _errs2;
  if (vErrors !== null) {
    if (_errs2) {
      vErrors.length = _errs2;
    } else {
      vErrors = null;
    }
  }
  let ifClause0;
  if (_valid0) {
    const _errs19 = errors;
    const _errs20 = errors;
    let valid4 = true;
    const _errs21 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.metrics !== void 0) {
        let data1 = data.metrics;
        if (!(data1 && typeof data1 == "object" && !Array.isArray(data1))) {
          const err14 = {};
          if (vErrors === null) {
            vErrors = [err14];
          } else {
            vErrors.push(err14);
          }
          errors++;
        }
        const _errs24 = errors;
        let valid6 = false;
        const _errs25 = errors;
        if (errors === _errs25) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing6;
            if (data1.totalTokens === void 0 && (missing6 = "totalTokens")) {
              const err15 = {};
              if (vErrors === null) {
                vErrors = [err15];
              } else {
                vErrors.push(err15);
              }
              errors++;
            }
          } else {
            const err16 = {};
            if (vErrors === null) {
              vErrors = [err16];
            } else {
              vErrors.push(err16);
            }
            errors++;
          }
        }
        var _valid3 = _errs25 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          var props1 = {};
          props1.totalTokens = true;
        }
        const _errs27 = errors;
        if (errors === _errs27) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing7;
            if (data1.totalAccumulatedTokens === void 0 && (missing7 = "totalAccumulatedTokens")) {
              const err17 = {};
              if (vErrors === null) {
                vErrors = [err17];
              } else {
                vErrors.push(err17);
              }
              errors++;
            }
          } else {
            const err18 = {};
            if (vErrors === null) {
              vErrors = [err18];
            } else {
              vErrors.push(err18);
            }
            errors++;
          }
        }
        var _valid3 = _errs27 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.totalAccumulatedTokens = true;
          }
        }
        const _errs29 = errors;
        if (errors === _errs29) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing8;
            if (data1.inputTokens === void 0 && (missing8 = "inputTokens") || data1.outputTokens === void 0 && (missing8 = "outputTokens")) {
              const err19 = {};
              if (vErrors === null) {
                vErrors = [err19];
              } else {
                vErrors.push(err19);
              }
              errors++;
            }
          } else {
            const err20 = {};
            if (vErrors === null) {
              vErrors = [err20];
            } else {
              vErrors.push(err20);
            }
            errors++;
          }
        }
        var _valid3 = _errs29 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.inputTokens = true;
            props1.outputTokens = true;
          }
        }
        const _errs31 = errors;
        if (errors === _errs31) {
          if (data1 && typeof data1 == "object" && !Array.isArray(data1)) {
            let missing9;
            if (data1.totalInputTokens === void 0 && (missing9 = "totalInputTokens") || data1.totalOutputTokens === void 0 && (missing9 = "totalOutputTokens")) {
              const err21 = {};
              if (vErrors === null) {
                vErrors = [err21];
              } else {
                vErrors.push(err21);
              }
              errors++;
            }
          } else {
            const err22 = {};
            if (vErrors === null) {
              vErrors = [err22];
            } else {
              vErrors.push(err22);
            }
            errors++;
          }
        }
        var _valid3 = _errs31 === errors;
        valid6 = valid6 || _valid3;
        if (_valid3) {
          if (props1 !== true) {
            props1 = props1 || {};
            props1.totalInputTokens = true;
            props1.totalOutputTokens = true;
          }
        }
        if (!valid6) {
          const err23 = {};
          if (vErrors === null) {
            vErrors = [err23];
          } else {
            vErrors.push(err23);
          }
          errors++;
        } else {
          errors = _errs24;
          if (vErrors !== null) {
            if (_errs24) {
              vErrors.length = _errs24;
            } else {
              vErrors = null;
            }
          }
        }
      }
    }
    var _valid2 = _errs21 === errors;
    errors = _errs20;
    if (vErrors !== null) {
      if (_errs20) {
        vErrors.length = _errs20;
      } else {
        vErrors = null;
      }
    }
    let ifClause1;
    if (_valid2) {
      const _errs33 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.availability !== void 0) {
          let data2 = data.availability;
          const _errs34 = errors;
          if (errors === _errs34) {
            if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
              if (data2.tokens !== void 0) {
                let data3 = data2.tokens;
                const _errs36 = errors;
                if (errors === _errs36) {
                  if (data3 && typeof data3 == "object" && !Array.isArray(data3)) {
                    if (data3.state !== void 0) {
                      if ("available" !== data3.state) {
                        validate29.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/then/then/properties/availability/properties/tokens/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" }];
                        return false;
                      }
                    }
                  } else {
                    validate29.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/then/then/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
              }
            } else {
              validate29.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/then/then/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
              return false;
            }
          }
        }
      }
      var _valid2 = _errs33 === errors;
      valid4 = _valid2;
      if (valid4) {
        var props2 = {};
        props2.availability = true;
        props2.metrics = true;
      }
      ifClause1 = "then";
    } else {
      const _errs39 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.availability !== void 0) {
          let data5 = data.availability;
          const _errs40 = errors;
          if (errors === _errs40) {
            if (data5 && typeof data5 == "object" && !Array.isArray(data5)) {
              if (data5.tokens !== void 0) {
                let data6 = data5.tokens;
                const _errs42 = errors;
                if (errors === _errs42) {
                  if (data6 && typeof data6 == "object" && !Array.isArray(data6)) {
                    if (data6.state !== void 0) {
                      const _errs44 = errors;
                      if ("source_unavailable" !== data6.state) {
                        validate29.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" }];
                        return false;
                      }
                      var valid12 = _errs44 === errors;
                    } else {
                      var valid12 = true;
                    }
                    if (valid12) {
                      if (data6.reason !== void 0) {
                        const _errs45 = errors;
                        if ("partial_token_metrics" !== data6.reason) {
                          validate29.errors = [{ instancePath: instancePath + "/availability/tokens/reason", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/properties/reason/const", keyword: "const", params: { allowedValue: "partial_token_metrics" }, message: "must be equal to constant" }];
                          return false;
                        }
                        var valid12 = _errs45 === errors;
                      } else {
                        var valid12 = true;
                      }
                    }
                  } else {
                    validate29.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/then/else/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
              }
            } else {
              validate29.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/then/else/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
              return false;
            }
          }
        }
      }
      var _valid2 = _errs39 === errors;
      valid4 = _valid2;
      if (valid4) {
        if (props2 !== true) {
          props2 = props2 || {};
          props2.availability = true;
        }
      }
      ifClause1 = "else";
    }
    if (!valid4) {
      const err24 = { instancePath, schemaPath: "#/allOf/0/then/if", keyword: "if", params: { failingKeyword: ifClause1 }, message: 'must match "' + ifClause1 + '" schema' };
      if (vErrors === null) {
        vErrors = [err24];
      } else {
        vErrors.push(err24);
      }
      errors++;
      validate29.errors = vErrors;
      return false;
    }
    var _valid0 = _errs19 === errors;
    valid1 = _valid0;
    if (valid1) {
      if (props2 !== true) {
        props2 = props2 || {};
        props2.metrics = true;
      }
    }
    ifClause0 = "then";
  } else {
    const _errs46 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      if (data.availability !== void 0) {
        let data9 = data.availability;
        const _errs47 = errors;
        if (errors === _errs47) {
          if (data9 && typeof data9 == "object" && !Array.isArray(data9)) {
            if (data9.tokens !== void 0) {
              let data10 = data9.tokens;
              const _errs49 = errors;
              if (errors === _errs49) {
                if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                  if (data10.state !== void 0) {
                    const _errs52 = errors;
                    const _errs53 = errors;
                    if ("available" !== data10.state) {
                      const err25 = {};
                      if (vErrors === null) {
                        vErrors = [err25];
                      } else {
                        vErrors.push(err25);
                      }
                      errors++;
                    }
                    var valid16 = _errs53 === errors;
                    if (valid16) {
                      validate29.errors = [{ instancePath: instancePath + "/availability/tokens/state", schemaPath: "#/allOf/0/else/properties/availability/properties/tokens/properties/state/not", keyword: "not", params: {}, message: "must NOT be valid" }];
                      return false;
                    } else {
                      errors = _errs52;
                      if (vErrors !== null) {
                        if (_errs52) {
                          vErrors.length = _errs52;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                  }
                } else {
                  validate29.errors = [{ instancePath: instancePath + "/availability/tokens", schemaPath: "#/allOf/0/else/properties/availability/properties/tokens/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
            }
          } else {
            validate29.errors = [{ instancePath: instancePath + "/availability", schemaPath: "#/allOf/0/else/properties/availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
            return false;
          }
        }
      }
    }
    var _valid0 = _errs46 === errors;
    valid1 = _valid0;
    if (valid1) {
      if (props2 !== true) {
        props2 = props2 || {};
        props2.availability = true;
      }
    }
    ifClause0 = "else";
  }
  if (!valid1) {
    const err26 = { instancePath, schemaPath: "#/allOf/0/if", keyword: "if", params: { failingKeyword: ifClause0 }, message: 'must match "' + ifClause0 + '" schema' };
    if (vErrors === null) {
      vErrors = [err26];
    } else {
      vErrors.push(err26);
    }
    errors++;
    validate29.errors = vErrors;
    return false;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing10;
      if (data.schemaVersion === void 0 && (missing10 = "schemaVersion") || data.traceId === void 0 && (missing10 = "traceId") || data.spanId === void 0 && (missing10 = "spanId") || data.parentSpanId === void 0 && (missing10 = "parentSpanId") || data.kind === void 0 && (missing10 = "kind") || data.name === void 0 && (missing10 = "name") || data.status === void 0 && (missing10 = "status") || data.startTimeUnixMs === void 0 && (missing10 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing10 = "endTimeUnixMs") || data.repo === void 0 && (missing10 = "repo") || data.agent === void 0 && (missing10 = "agent") || data.availability === void 0 && (missing10 = "availability") || data.attributes === void 0 && (missing10 = "attributes") || data.metrics === void 0 && (missing10 = "metrics") || data.cost === void 0 && (missing10 = "cost")) {
        validate29.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing10 }, message: "must have required property '" + missing10 + "'" }];
        return false;
      } else {
        const _errs54 = errors;
        for (const key0 in data) {
          if (!func1.call(schema47.properties, key0)) {
            validate29.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs54 === errors) {
          if (data.schemaVersion !== void 0) {
            const _errs55 = errors;
            if (typeof data.schemaVersion !== "string") {
              validate29.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
              return false;
            }
            var valid17 = _errs55 === errors;
          } else {
            var valid17 = true;
          }
          if (valid17) {
            if (data.traceId !== void 0) {
              const _errs57 = errors;
              if (typeof data.traceId !== "string") {
                validate29.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/properties/traceId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid17 = _errs57 === errors;
            } else {
              var valid17 = true;
            }
            if (valid17) {
              if (data.spanId !== void 0) {
                const _errs59 = errors;
                if (typeof data.spanId !== "string") {
                  validate29.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/properties/spanId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                var valid17 = _errs59 === errors;
              } else {
                var valid17 = true;
              }
              if (valid17) {
                if (data.parentSpanId !== void 0) {
                  let data15 = data.parentSpanId;
                  const _errs61 = errors;
                  if (typeof data15 !== "string" && data15 !== null) {
                    validate29.errors = [{ instancePath: instancePath + "/parentSpanId", schemaPath: "#/properties/parentSpanId/type", keyword: "type", params: { type: schema47.properties.parentSpanId.type }, message: "must be string,null" }];
                    return false;
                  }
                  var valid17 = _errs61 === errors;
                } else {
                  var valid17 = true;
                }
                if (valid17) {
                  if (data.kind !== void 0) {
                    const _errs63 = errors;
                    if (typeof data.kind !== "string") {
                      validate29.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    var valid17 = _errs63 === errors;
                  } else {
                    var valid17 = true;
                  }
                  if (valid17) {
                    if (data.name !== void 0) {
                      const _errs65 = errors;
                      if (typeof data.name !== "string") {
                        validate29.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                      var valid17 = _errs65 === errors;
                    } else {
                      var valid17 = true;
                    }
                    if (valid17) {
                      if (data.status !== void 0) {
                        const _errs67 = errors;
                        if (typeof data.status !== "string") {
                          validate29.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid17 = _errs67 === errors;
                      } else {
                        var valid17 = true;
                      }
                      if (valid17) {
                        if (data.startTimeUnixMs !== void 0) {
                          let data19 = data.startTimeUnixMs;
                          const _errs69 = errors;
                          if (!(typeof data19 == "number" && isFinite(data19))) {
                            validate29.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                            return false;
                          }
                          var valid17 = _errs69 === errors;
                        } else {
                          var valid17 = true;
                        }
                        if (valid17) {
                          if (data.endTimeUnixMs !== void 0) {
                            let data20 = data.endTimeUnixMs;
                            const _errs71 = errors;
                            if (!(typeof data20 == "number" && isFinite(data20)) && data20 !== null) {
                              validate29.errors = [{ instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/type", keyword: "type", params: { type: schema47.properties.endTimeUnixMs.type }, message: "must be number,null" }];
                              return false;
                            }
                            var valid17 = _errs71 === errors;
                          } else {
                            var valid17 = true;
                          }
                          if (valid17) {
                            if (data.repo !== void 0) {
                              const _errs73 = errors;
                              if (typeof data.repo !== "string") {
                                validate29.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              var valid17 = _errs73 === errors;
                            } else {
                              var valid17 = true;
                            }
                            if (valid17) {
                              if (data.agent !== void 0) {
                                let data22 = data.agent;
                                const _errs75 = errors;
                                const _errs76 = errors;
                                if (errors === _errs76) {
                                  if (data22 && typeof data22 == "object" && !Array.isArray(data22)) {
                                    const _errs78 = errors;
                                    for (const key1 in data22) {
                                      if (!(key1 === "name" || key1 === "model" || key1 === "version")) {
                                        validate29.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                        return false;
                                        break;
                                      }
                                    }
                                    if (_errs78 === errors) {
                                      if (data22.name !== void 0) {
                                        const _errs79 = errors;
                                        if (typeof data22.name !== "string") {
                                          validate29.errors = [{ instancePath: instancePath + "/agent/name", schemaPath: "#/$defs/agent/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                        var valid19 = _errs79 === errors;
                                      } else {
                                        var valid19 = true;
                                      }
                                      if (valid19) {
                                        if (data22.model !== void 0) {
                                          const _errs81 = errors;
                                          if (typeof data22.model !== "string") {
                                            validate29.errors = [{ instancePath: instancePath + "/agent/model", schemaPath: "#/$defs/agent/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                            return false;
                                          }
                                          var valid19 = _errs81 === errors;
                                        } else {
                                          var valid19 = true;
                                        }
                                        if (valid19) {
                                          if (data22.version !== void 0) {
                                            const _errs83 = errors;
                                            if (typeof data22.version !== "string") {
                                              validate29.errors = [{ instancePath: instancePath + "/agent/version", schemaPath: "#/$defs/agent/properties/version/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                              return false;
                                            }
                                            var valid19 = _errs83 === errors;
                                          } else {
                                            var valid19 = true;
                                          }
                                        }
                                      }
                                    }
                                  } else {
                                    validate29.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                    return false;
                                  }
                                }
                                var valid17 = _errs75 === errors;
                              } else {
                                var valid17 = true;
                              }
                              if (valid17) {
                                if (data.availability !== void 0) {
                                  const _errs85 = errors;
                                  if (!validate30(data.availability, { instancePath: instancePath + "/availability", parentData: data, parentDataProperty: "availability", rootData, dynamicAnchors })) {
                                    vErrors = vErrors === null ? validate30.errors : vErrors.concat(validate30.errors);
                                    errors = vErrors.length;
                                  }
                                  var valid17 = _errs85 === errors;
                                } else {
                                  var valid17 = true;
                                }
                                if (valid17) {
                                  if (data.sessionId !== void 0) {
                                    const _errs86 = errors;
                                    if (typeof data.sessionId !== "string") {
                                      validate29.errors = [{ instancePath: instancePath + "/sessionId", schemaPath: "#/properties/sessionId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    var valid17 = _errs86 === errors;
                                  } else {
                                    var valid17 = true;
                                  }
                                  if (valid17) {
                                    if (data.turnId !== void 0) {
                                      const _errs88 = errors;
                                      if (typeof data.turnId !== "string") {
                                        validate29.errors = [{ instancePath: instancePath + "/turnId", schemaPath: "#/properties/turnId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      var valid17 = _errs88 === errors;
                                    } else {
                                      var valid17 = true;
                                    }
                                    if (valid17) {
                                      if (data.toolName !== void 0) {
                                        const _errs90 = errors;
                                        if (typeof data.toolName !== "string") {
                                          validate29.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                        var valid17 = _errs90 === errors;
                                      } else {
                                        var valid17 = true;
                                      }
                                      if (valid17) {
                                        if (data.attributes !== void 0) {
                                          const _errs92 = errors;
                                          if (!validate32(data.attributes, { instancePath: instancePath + "/attributes", parentData: data, parentDataProperty: "attributes", rootData, dynamicAnchors })) {
                                            vErrors = vErrors === null ? validate32.errors : vErrors.concat(validate32.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid17 = _errs92 === errors;
                                        } else {
                                          var valid17 = true;
                                        }
                                        if (valid17) {
                                          if (data.metrics !== void 0) {
                                            let data31 = data.metrics;
                                            const _errs93 = errors;
                                            const _errs94 = errors;
                                            if (errors === _errs94) {
                                              if (data31 && typeof data31 == "object" && !Array.isArray(data31)) {
                                                const _errs96 = errors;
                                                for (const key2 in data31) {
                                                  if (!func1.call(schema71.properties, key2)) {
                                                    validate29.errors = [{ instancePath: instancePath + "/metrics", schemaPath: "#/$defs/metrics/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                                    return false;
                                                    break;
                                                  }
                                                }
                                                if (_errs96 === errors) {
                                                  if (data31.inputTokens !== void 0) {
                                                    let data32 = data31.inputTokens;
                                                    const _errs97 = errors;
                                                    if (!(typeof data32 == "number" && isFinite(data32))) {
                                                      validate29.errors = [{ instancePath: instancePath + "/metrics/inputTokens", schemaPath: "#/$defs/metrics/properties/inputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                      return false;
                                                    }
                                                    var valid21 = _errs97 === errors;
                                                  } else {
                                                    var valid21 = true;
                                                  }
                                                  if (valid21) {
                                                    if (data31.outputTokens !== void 0) {
                                                      let data33 = data31.outputTokens;
                                                      const _errs99 = errors;
                                                      if (!(typeof data33 == "number" && isFinite(data33))) {
                                                        validate29.errors = [{ instancePath: instancePath + "/metrics/outputTokens", schemaPath: "#/$defs/metrics/properties/outputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                        return false;
                                                      }
                                                      var valid21 = _errs99 === errors;
                                                    } else {
                                                      var valid21 = true;
                                                    }
                                                    if (valid21) {
                                                      if (data31.cachedInputTokens !== void 0) {
                                                        let data34 = data31.cachedInputTokens;
                                                        const _errs101 = errors;
                                                        if (!(typeof data34 == "number" && isFinite(data34))) {
                                                          validate29.errors = [{ instancePath: instancePath + "/metrics/cachedInputTokens", schemaPath: "#/$defs/metrics/properties/cachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                          return false;
                                                        }
                                                        var valid21 = _errs101 === errors;
                                                      } else {
                                                        var valid21 = true;
                                                      }
                                                      if (valid21) {
                                                        if (data31.cacheCreationInputTokens !== void 0) {
                                                          let data35 = data31.cacheCreationInputTokens;
                                                          const _errs103 = errors;
                                                          if (!(typeof data35 == "number" && isFinite(data35))) {
                                                            validate29.errors = [{ instancePath: instancePath + "/metrics/cacheCreationInputTokens", schemaPath: "#/$defs/metrics/properties/cacheCreationInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                            return false;
                                                          }
                                                          var valid21 = _errs103 === errors;
                                                        } else {
                                                          var valid21 = true;
                                                        }
                                                        if (valid21) {
                                                          if (data31.reasoningOutputTokens !== void 0) {
                                                            let data36 = data31.reasoningOutputTokens;
                                                            const _errs105 = errors;
                                                            if (!(typeof data36 == "number" && isFinite(data36))) {
                                                              validate29.errors = [{ instancePath: instancePath + "/metrics/reasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/reasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                              return false;
                                                            }
                                                            var valid21 = _errs105 === errors;
                                                          } else {
                                                            var valid21 = true;
                                                          }
                                                          if (valid21) {
                                                            if (data31.totalTokens !== void 0) {
                                                              let data37 = data31.totalTokens;
                                                              const _errs107 = errors;
                                                              if (!(typeof data37 == "number" && isFinite(data37))) {
                                                                validate29.errors = [{ instancePath: instancePath + "/metrics/totalTokens", schemaPath: "#/$defs/metrics/properties/totalTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                return false;
                                                              }
                                                              var valid21 = _errs107 === errors;
                                                            } else {
                                                              var valid21 = true;
                                                            }
                                                            if (valid21) {
                                                              if (data31.latencyMs !== void 0) {
                                                                let data38 = data31.latencyMs;
                                                                const _errs109 = errors;
                                                                if (!(typeof data38 == "number" && isFinite(data38))) {
                                                                  validate29.errors = [{ instancePath: instancePath + "/metrics/latencyMs", schemaPath: "#/$defs/metrics/properties/latencyMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                  return false;
                                                                }
                                                                var valid21 = _errs109 === errors;
                                                              } else {
                                                                var valid21 = true;
                                                              }
                                                              if (valid21) {
                                                                if (data31.durationMs !== void 0) {
                                                                  let data39 = data31.durationMs;
                                                                  const _errs111 = errors;
                                                                  if (!(typeof data39 == "number" && isFinite(data39))) {
                                                                    validate29.errors = [{ instancePath: instancePath + "/metrics/durationMs", schemaPath: "#/$defs/metrics/properties/durationMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                    return false;
                                                                  }
                                                                  var valid21 = _errs111 === errors;
                                                                } else {
                                                                  var valid21 = true;
                                                                }
                                                                if (valid21) {
                                                                  if (data31.totalInputTokens !== void 0) {
                                                                    let data40 = data31.totalInputTokens;
                                                                    const _errs113 = errors;
                                                                    if (!(typeof data40 == "number" && isFinite(data40))) {
                                                                      validate29.errors = [{ instancePath: instancePath + "/metrics/totalInputTokens", schemaPath: "#/$defs/metrics/properties/totalInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                      return false;
                                                                    }
                                                                    var valid21 = _errs113 === errors;
                                                                  } else {
                                                                    var valid21 = true;
                                                                  }
                                                                  if (valid21) {
                                                                    if (data31.totalOutputTokens !== void 0) {
                                                                      let data41 = data31.totalOutputTokens;
                                                                      const _errs115 = errors;
                                                                      if (!(typeof data41 == "number" && isFinite(data41))) {
                                                                        validate29.errors = [{ instancePath: instancePath + "/metrics/totalOutputTokens", schemaPath: "#/$defs/metrics/properties/totalOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                        return false;
                                                                      }
                                                                      var valid21 = _errs115 === errors;
                                                                    } else {
                                                                      var valid21 = true;
                                                                    }
                                                                    if (valid21) {
                                                                      if (data31.totalCachedInputTokens !== void 0) {
                                                                        let data42 = data31.totalCachedInputTokens;
                                                                        const _errs117 = errors;
                                                                        if (!(typeof data42 == "number" && isFinite(data42))) {
                                                                          validate29.errors = [{ instancePath: instancePath + "/metrics/totalCachedInputTokens", schemaPath: "#/$defs/metrics/properties/totalCachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                          return false;
                                                                        }
                                                                        var valid21 = _errs117 === errors;
                                                                      } else {
                                                                        var valid21 = true;
                                                                      }
                                                                      if (valid21) {
                                                                        if (data31.totalReasoningOutputTokens !== void 0) {
                                                                          let data43 = data31.totalReasoningOutputTokens;
                                                                          const _errs119 = errors;
                                                                          if (!(typeof data43 == "number" && isFinite(data43))) {
                                                                            validate29.errors = [{ instancePath: instancePath + "/metrics/totalReasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/totalReasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                            return false;
                                                                          }
                                                                          var valid21 = _errs119 === errors;
                                                                        } else {
                                                                          var valid21 = true;
                                                                        }
                                                                        if (valid21) {
                                                                          if (data31.totalAccumulatedTokens !== void 0) {
                                                                            let data44 = data31.totalAccumulatedTokens;
                                                                            const _errs121 = errors;
                                                                            if (!(typeof data44 == "number" && isFinite(data44))) {
                                                                              validate29.errors = [{ instancePath: instancePath + "/metrics/totalAccumulatedTokens", schemaPath: "#/$defs/metrics/properties/totalAccumulatedTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                              return false;
                                                                            }
                                                                            var valid21 = _errs121 === errors;
                                                                          } else {
                                                                            var valid21 = true;
                                                                          }
                                                                          if (valid21) {
                                                                            if (data31.contextWindowTokens !== void 0) {
                                                                              let data45 = data31.contextWindowTokens;
                                                                              const _errs123 = errors;
                                                                              if (!(typeof data45 == "number" && isFinite(data45))) {
                                                                                validate29.errors = [{ instancePath: instancePath + "/metrics/contextWindowTokens", schemaPath: "#/$defs/metrics/properties/contextWindowTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                                return false;
                                                                              }
                                                                              var valid21 = _errs123 === errors;
                                                                            } else {
                                                                              var valid21 = true;
                                                                            }
                                                                          }
                                                                        }
                                                                      }
                                                                    }
                                                                  }
                                                                }
                                                              }
                                                            }
                                                          }
                                                        }
                                                      }
                                                    }
                                                  }
                                                }
                                              } else {
                                                validate29.errors = [{ instancePath: instancePath + "/metrics", schemaPath: "#/$defs/metrics/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                                return false;
                                              }
                                            }
                                            var valid17 = _errs93 === errors;
                                          } else {
                                            var valid17 = true;
                                          }
                                          if (valid17) {
                                            if (data.estimatedCost !== void 0) {
                                              let data46 = data.estimatedCost;
                                              const _errs125 = errors;
                                              if (!(typeof data46 == "number" && isFinite(data46))) {
                                                validate29.errors = [{ instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                return false;
                                              }
                                              var valid17 = _errs125 === errors;
                                            } else {
                                              var valid17 = true;
                                            }
                                            if (valid17) {
                                              if (data.cost !== void 0) {
                                                const _errs127 = errors;
                                                if (!validate21(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                                                  vErrors = vErrors === null ? validate21.errors : vErrors.concat(validate21.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid17 = _errs127 === errors;
                                              } else {
                                                var valid17 = true;
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate29.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate29.errors = vErrors;
  return errors === 0;
}
validate29.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
function validate20(data, { instancePath = "", parentData, parentDataProperty, rootData = data, dynamicAnchors = {} } = {}) {
  let vErrors = null;
  let errors = 0;
  const evaluated0 = validate20.evaluated;
  if (evaluated0.dynamicProps) {
    evaluated0.props = void 0;
  }
  if (evaluated0.dynamicItems) {
    evaluated0.items = void 0;
  }
  if (errors === 0) {
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.schemaVersion === void 0 && (missing0 = "schemaVersion") || data.generatedAt === void 0 && (missing0 = "generatedAt") || data.title === void 0 && (missing0 = "title") || data.summary === void 0 && (missing0 = "summary") || data.cost === void 0 && (missing0 = "cost") || data.filters === void 0 && (missing0 = "filters") || data.traces === void 0 && (missing0 = "traces") || data.spans === void 0 && (missing0 = "spans")) {
        validate20.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
        return false;
      } else {
        const _errs1 = errors;
        for (const key0 in data) {
          if (!(key0 === "schemaVersion" || key0 === "generatedAt" || key0 === "title" || key0 === "summary" || key0 === "cost" || key0 === "filters" || key0 === "traces" || key0 === "spans")) {
            validate20.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.schemaVersion !== void 0) {
            const _errs2 = errors;
            if ("agent_observability.report.v2" !== data.schemaVersion) {
              validate20.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/const", keyword: "const", params: { allowedValue: "agent_observability.report.v2" }, message: "must be equal to constant" }];
              return false;
            }
            var valid0 = _errs2 === errors;
          } else {
            var valid0 = true;
          }
          if (valid0) {
            if (data.generatedAt !== void 0) {
              const _errs3 = errors;
              if (typeof data.generatedAt !== "string") {
                validate20.errors = [{ instancePath: instancePath + "/generatedAt", schemaPath: "#/properties/generatedAt/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid0 = _errs3 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.title !== void 0) {
                const _errs5 = errors;
                if (typeof data.title !== "string") {
                  validate20.errors = [{ instancePath: instancePath + "/title", schemaPath: "#/properties/title/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                var valid0 = _errs5 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.summary !== void 0) {
                  let data3 = data.summary;
                  const _errs7 = errors;
                  const _errs8 = errors;
                  if (errors === _errs8) {
                    if (data3 && typeof data3 == "object" && !Array.isArray(data3)) {
                      let missing1;
                      if (data3.generatedSpans === void 0 && (missing1 = "generatedSpans") || data3.sessions === void 0 && (missing1 = "sessions") || data3.turns === void 0 && (missing1 = "turns") || data3.llmRequests === void 0 && (missing1 = "llmRequests") || data3.toolExecutions === void 0 && (missing1 = "toolExecutions") || data3.errors === void 0 && (missing1 = "errors") || data3.inputTokens === void 0 && (missing1 = "inputTokens") || data3.outputTokens === void 0 && (missing1 = "outputTokens") || data3.cachedInputTokens === void 0 && (missing1 = "cachedInputTokens") || data3.cacheCreationInputTokens === void 0 && (missing1 = "cacheCreationInputTokens") || data3.reasoningOutputTokens === void 0 && (missing1 = "reasoningOutputTokens") || data3.latencyMs === void 0 && (missing1 = "latencyMs") || data3.durationMs === void 0 && (missing1 = "durationMs") || data3.estimatedCost === void 0 && (missing1 = "estimatedCost")) {
                        validate20.errors = [{ instancePath: instancePath + "/summary", schemaPath: "#/$defs/summary/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                        return false;
                      } else {
                        const _errs10 = errors;
                        for (const key1 in data3) {
                          if (!func1.call(schema32.properties, key1)) {
                            validate20.errors = [{ instancePath: instancePath + "/summary", schemaPath: "#/$defs/summary/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                            return false;
                            break;
                          }
                        }
                        if (_errs10 === errors) {
                          if (data3.generatedSpans !== void 0) {
                            let data4 = data3.generatedSpans;
                            const _errs11 = errors;
                            if (!(typeof data4 == "number" && isFinite(data4))) {
                              validate20.errors = [{ instancePath: instancePath + "/summary/generatedSpans", schemaPath: "#/$defs/summary/properties/generatedSpans/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                              return false;
                            }
                            var valid2 = _errs11 === errors;
                          } else {
                            var valid2 = true;
                          }
                          if (valid2) {
                            if (data3.sessions !== void 0) {
                              let data5 = data3.sessions;
                              const _errs13 = errors;
                              if (!(typeof data5 == "number" && isFinite(data5))) {
                                validate20.errors = [{ instancePath: instancePath + "/summary/sessions", schemaPath: "#/$defs/summary/properties/sessions/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                return false;
                              }
                              var valid2 = _errs13 === errors;
                            } else {
                              var valid2 = true;
                            }
                            if (valid2) {
                              if (data3.turns !== void 0) {
                                let data6 = data3.turns;
                                const _errs15 = errors;
                                if (!(typeof data6 == "number" && isFinite(data6))) {
                                  validate20.errors = [{ instancePath: instancePath + "/summary/turns", schemaPath: "#/$defs/summary/properties/turns/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                  return false;
                                }
                                var valid2 = _errs15 === errors;
                              } else {
                                var valid2 = true;
                              }
                              if (valid2) {
                                if (data3.llmRequests !== void 0) {
                                  let data7 = data3.llmRequests;
                                  const _errs17 = errors;
                                  if (!(typeof data7 == "number" && isFinite(data7))) {
                                    validate20.errors = [{ instancePath: instancePath + "/summary/llmRequests", schemaPath: "#/$defs/summary/properties/llmRequests/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                    return false;
                                  }
                                  var valid2 = _errs17 === errors;
                                } else {
                                  var valid2 = true;
                                }
                                if (valid2) {
                                  if (data3.toolExecutions !== void 0) {
                                    let data8 = data3.toolExecutions;
                                    const _errs19 = errors;
                                    if (!(typeof data8 == "number" && isFinite(data8))) {
                                      validate20.errors = [{ instancePath: instancePath + "/summary/toolExecutions", schemaPath: "#/$defs/summary/properties/toolExecutions/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                      return false;
                                    }
                                    var valid2 = _errs19 === errors;
                                  } else {
                                    var valid2 = true;
                                  }
                                  if (valid2) {
                                    if (data3.errors !== void 0) {
                                      let data9 = data3.errors;
                                      const _errs21 = errors;
                                      if (!(typeof data9 == "number" && isFinite(data9))) {
                                        validate20.errors = [{ instancePath: instancePath + "/summary/errors", schemaPath: "#/$defs/summary/properties/errors/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                        return false;
                                      }
                                      var valid2 = _errs21 === errors;
                                    } else {
                                      var valid2 = true;
                                    }
                                    if (valid2) {
                                      if (data3.inputTokens !== void 0) {
                                        let data10 = data3.inputTokens;
                                        const _errs23 = errors;
                                        if (!(typeof data10 == "number" && isFinite(data10))) {
                                          validate20.errors = [{ instancePath: instancePath + "/summary/inputTokens", schemaPath: "#/$defs/summary/properties/inputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                          return false;
                                        }
                                        var valid2 = _errs23 === errors;
                                      } else {
                                        var valid2 = true;
                                      }
                                      if (valid2) {
                                        if (data3.outputTokens !== void 0) {
                                          let data11 = data3.outputTokens;
                                          const _errs25 = errors;
                                          if (!(typeof data11 == "number" && isFinite(data11))) {
                                            validate20.errors = [{ instancePath: instancePath + "/summary/outputTokens", schemaPath: "#/$defs/summary/properties/outputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                            return false;
                                          }
                                          var valid2 = _errs25 === errors;
                                        } else {
                                          var valid2 = true;
                                        }
                                        if (valid2) {
                                          if (data3.cachedInputTokens !== void 0) {
                                            let data12 = data3.cachedInputTokens;
                                            const _errs27 = errors;
                                            if (!(typeof data12 == "number" && isFinite(data12))) {
                                              validate20.errors = [{ instancePath: instancePath + "/summary/cachedInputTokens", schemaPath: "#/$defs/summary/properties/cachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                              return false;
                                            }
                                            var valid2 = _errs27 === errors;
                                          } else {
                                            var valid2 = true;
                                          }
                                          if (valid2) {
                                            if (data3.cacheCreationInputTokens !== void 0) {
                                              let data13 = data3.cacheCreationInputTokens;
                                              const _errs29 = errors;
                                              if (!(typeof data13 == "number" && isFinite(data13))) {
                                                validate20.errors = [{ instancePath: instancePath + "/summary/cacheCreationInputTokens", schemaPath: "#/$defs/summary/properties/cacheCreationInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                return false;
                                              }
                                              var valid2 = _errs29 === errors;
                                            } else {
                                              var valid2 = true;
                                            }
                                            if (valid2) {
                                              if (data3.reasoningOutputTokens !== void 0) {
                                                let data14 = data3.reasoningOutputTokens;
                                                const _errs31 = errors;
                                                if (!(typeof data14 == "number" && isFinite(data14))) {
                                                  validate20.errors = [{ instancePath: instancePath + "/summary/reasoningOutputTokens", schemaPath: "#/$defs/summary/properties/reasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                  return false;
                                                }
                                                var valid2 = _errs31 === errors;
                                              } else {
                                                var valid2 = true;
                                              }
                                              if (valid2) {
                                                if (data3.latencyMs !== void 0) {
                                                  let data15 = data3.latencyMs;
                                                  const _errs33 = errors;
                                                  if (!(typeof data15 == "number" && isFinite(data15))) {
                                                    validate20.errors = [{ instancePath: instancePath + "/summary/latencyMs", schemaPath: "#/$defs/summary/properties/latencyMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                    return false;
                                                  }
                                                  var valid2 = _errs33 === errors;
                                                } else {
                                                  var valid2 = true;
                                                }
                                                if (valid2) {
                                                  if (data3.durationMs !== void 0) {
                                                    let data16 = data3.durationMs;
                                                    const _errs35 = errors;
                                                    if (!(typeof data16 == "number" && isFinite(data16))) {
                                                      validate20.errors = [{ instancePath: instancePath + "/summary/durationMs", schemaPath: "#/$defs/summary/properties/durationMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                      return false;
                                                    }
                                                    var valid2 = _errs35 === errors;
                                                  } else {
                                                    var valid2 = true;
                                                  }
                                                  if (valid2) {
                                                    if (data3.estimatedCost !== void 0) {
                                                      let data17 = data3.estimatedCost;
                                                      const _errs37 = errors;
                                                      if (!(typeof data17 == "number" && isFinite(data17))) {
                                                        validate20.errors = [{ instancePath: instancePath + "/summary/estimatedCost", schemaPath: "#/$defs/summary/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                        return false;
                                                      }
                                                      var valid2 = _errs37 === errors;
                                                    } else {
                                                      var valid2 = true;
                                                    }
                                                  }
                                                }
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    } else {
                      validate20.errors = [{ instancePath: instancePath + "/summary", schemaPath: "#/$defs/summary/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                      return false;
                    }
                  }
                  var valid0 = _errs7 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.cost !== void 0) {
                    const _errs39 = errors;
                    if (!validate21(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                      vErrors = vErrors === null ? validate21.errors : vErrors.concat(validate21.errors);
                      errors = vErrors.length;
                    }
                    var valid0 = _errs39 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.filters !== void 0) {
                      const _errs40 = errors;
                      if (!validate25(data.filters, { instancePath: instancePath + "/filters", parentData: data, parentDataProperty: "filters", rootData, dynamicAnchors })) {
                        vErrors = vErrors === null ? validate25.errors : vErrors.concat(validate25.errors);
                        errors = vErrors.length;
                      }
                      var valid0 = _errs40 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.traces !== void 0) {
                        let data20 = data.traces;
                        const _errs41 = errors;
                        if (errors === _errs41) {
                          if (Array.isArray(data20)) {
                            var valid3 = true;
                            const len0 = data20.length;
                            for (let i0 = 0; i0 < len0; i0++) {
                              const _errs43 = errors;
                              if (!validate27(data20[i0], { instancePath: instancePath + "/traces/" + i0, parentData: data20, parentDataProperty: i0, rootData, dynamicAnchors })) {
                                vErrors = vErrors === null ? validate27.errors : vErrors.concat(validate27.errors);
                                errors = vErrors.length;
                              }
                              var valid3 = _errs43 === errors;
                              if (!valid3) {
                                break;
                              }
                            }
                          } else {
                            validate20.errors = [{ instancePath: instancePath + "/traces", schemaPath: "#/properties/traces/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                            return false;
                          }
                        }
                        var valid0 = _errs41 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.spans !== void 0) {
                          let data22 = data.spans;
                          const _errs44 = errors;
                          if (errors === _errs44) {
                            if (Array.isArray(data22)) {
                              var valid4 = true;
                              const len1 = data22.length;
                              for (let i1 = 0; i1 < len1; i1++) {
                                const _errs46 = errors;
                                if (!validate29(data22[i1], { instancePath: instancePath + "/spans/" + i1, parentData: data22, parentDataProperty: i1, rootData, dynamicAnchors })) {
                                  vErrors = vErrors === null ? validate29.errors : vErrors.concat(validate29.errors);
                                  errors = vErrors.length;
                                }
                                var valid4 = _errs46 === errors;
                                if (!valid4) {
                                  break;
                                }
                              }
                            } else {
                              validate20.errors = [{ instancePath: instancePath + "/spans", schemaPath: "#/properties/spans/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                              return false;
                            }
                          }
                          var valid0 = _errs44 === errors;
                        } else {
                          var valid0 = true;
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    } else {
      validate20.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
      return false;
    }
  }
  validate20.errors = vErrors;
  return errors === 0;
}
validate20.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
export {
  validate_report_dto_v2_generated_default as default,
  validate
};
