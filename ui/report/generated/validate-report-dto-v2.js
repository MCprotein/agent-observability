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
var schema50 = { "type": "object", "additionalProperties": false, "required": ["state", "reason"], "properties": { "state": { "enum": ["available", "source_unavailable", "withheld", "not_applicable", "private_lookup"] }, "reason": { "type": "string", "enum": ["reported_by_adapter", "derived_from_trace_context", "legacy_v1_report", "source_not_provided", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported", "local_opt_in_lookup_required", "withheld_by_privacy_policy"] } }, "oneOf": [{ "properties": { "state": { "const": "available" }, "reason": { "enum": ["reported_by_adapter", "derived_from_trace_context", "legacy_v1_report"] } } }, { "properties": { "state": { "const": "source_unavailable" }, "reason": { "enum": ["source_not_provided", "partial_token_metrics", "historical_codex_source_not_lookup_eligible", "codex_notify_turn_correlation_unavailable", "ambiguous_trace_repository", "legacy_v1_report"] } } }, { "properties": { "state": { "const": "not_applicable" }, "reason": { "enum": ["span_kind_not_model_backed", "span_kind_has_no_latency", "span_kind_has_no_token_usage", "claude_private_lookup_not_supported", "cursor_private_lookup_not_supported", "codex_span_not_notify_derived", "agent_private_lookup_not_supported"] } } }, { "properties": { "state": { "const": "private_lookup" }, "reason": { "const": "local_opt_in_lookup_required" } } }, { "properties": { "state": { "const": "withheld" }, "reason": { "const": "withheld_by_privacy_policy" } } }] };
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
            const _errs5 = errors;
            let valid2 = false;
            let passing0 = null;
            const _errs6 = errors;
            if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
              if (data0.state !== void 0) {
                const _errs7 = errors;
                if ("available" !== data0.state) {
                  const err0 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err0];
                  } else {
                    vErrors.push(err0);
                  }
                  errors++;
                }
                var valid3 = _errs7 === errors;
              } else {
                var valid3 = true;
              }
              if (valid3) {
                if (data0.reason !== void 0) {
                  let data2 = data0.reason;
                  const _errs8 = errors;
                  if (!(data2 === "reported_by_adapter" || data2 === "derived_from_trace_context" || data2 === "legacy_v1_report")) {
                    const err1 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                    if (vErrors === null) {
                      vErrors = [err1];
                    } else {
                      vErrors.push(err1);
                    }
                    errors++;
                  }
                  var valid3 = _errs8 === errors;
                } else {
                  var valid3 = true;
                }
              }
            }
            var _valid0 = _errs6 === errors;
            if (_valid0) {
              valid2 = true;
              passing0 = 0;
              var props0 = {};
              props0.state = true;
              props0.reason = true;
            }
            const _errs9 = errors;
            if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
              if (data0.state !== void 0) {
                const _errs10 = errors;
                if ("source_unavailable" !== data0.state) {
                  const err2 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                  if (vErrors === null) {
                    vErrors = [err2];
                  } else {
                    vErrors.push(err2);
                  }
                  errors++;
                }
                var valid4 = _errs10 === errors;
              } else {
                var valid4 = true;
              }
              if (valid4) {
                if (data0.reason !== void 0) {
                  let data4 = data0.reason;
                  const _errs11 = errors;
                  if (!(data4 === "source_not_provided" || data4 === "partial_token_metrics" || data4 === "historical_codex_source_not_lookup_eligible" || data4 === "codex_notify_turn_correlation_unavailable" || data4 === "ambiguous_trace_repository" || data4 === "legacy_v1_report")) {
                    const err3 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  }
                  var valid4 = _errs11 === errors;
                } else {
                  var valid4 = true;
                }
              }
            }
            var _valid0 = _errs9 === errors;
            if (_valid0 && valid2) {
              valid2 = false;
              passing0 = [passing0, 1];
            } else {
              if (_valid0) {
                valid2 = true;
                passing0 = 1;
                if (props0 !== true) {
                  props0 = props0 || {};
                  props0.state = true;
                  props0.reason = true;
                }
              }
              const _errs12 = errors;
              if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                if (data0.state !== void 0) {
                  const _errs13 = errors;
                  if ("not_applicable" !== data0.state) {
                    const err4 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err4];
                    } else {
                      vErrors.push(err4);
                    }
                    errors++;
                  }
                  var valid5 = _errs13 === errors;
                } else {
                  var valid5 = true;
                }
                if (valid5) {
                  if (data0.reason !== void 0) {
                    let data6 = data0.reason;
                    const _errs14 = errors;
                    if (!(data6 === "span_kind_not_model_backed" || data6 === "span_kind_has_no_latency" || data6 === "span_kind_has_no_token_usage" || data6 === "claude_private_lookup_not_supported" || data6 === "cursor_private_lookup_not_supported" || data6 === "codex_span_not_notify_derived" || data6 === "agent_private_lookup_not_supported")) {
                      const err5 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err5];
                      } else {
                        vErrors.push(err5);
                      }
                      errors++;
                    }
                    var valid5 = _errs14 === errors;
                  } else {
                    var valid5 = true;
                  }
                }
              }
              var _valid0 = _errs12 === errors;
              if (_valid0 && valid2) {
                valid2 = false;
                passing0 = [passing0, 2];
              } else {
                if (_valid0) {
                  valid2 = true;
                  passing0 = 2;
                  if (props0 !== true) {
                    props0 = props0 || {};
                    props0.state = true;
                    props0.reason = true;
                  }
                }
                const _errs15 = errors;
                if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                  if (data0.state !== void 0) {
                    const _errs16 = errors;
                    if ("private_lookup" !== data0.state) {
                      const err6 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err6];
                      } else {
                        vErrors.push(err6);
                      }
                      errors++;
                    }
                    var valid6 = _errs16 === errors;
                  } else {
                    var valid6 = true;
                  }
                  if (valid6) {
                    if (data0.reason !== void 0) {
                      const _errs17 = errors;
                      if ("local_opt_in_lookup_required" !== data0.reason) {
                        const err7 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err7];
                        } else {
                          vErrors.push(err7);
                        }
                        errors++;
                      }
                      var valid6 = _errs17 === errors;
                    } else {
                      var valid6 = true;
                    }
                  }
                }
                var _valid0 = _errs15 === errors;
                if (_valid0 && valid2) {
                  valid2 = false;
                  passing0 = [passing0, 3];
                } else {
                  if (_valid0) {
                    valid2 = true;
                    passing0 = 3;
                    if (props0 !== true) {
                      props0 = props0 || {};
                      props0.state = true;
                      props0.reason = true;
                    }
                  }
                  const _errs18 = errors;
                  if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                    if (data0.state !== void 0) {
                      const _errs19 = errors;
                      if ("withheld" !== data0.state) {
                        const err8 = { instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err8];
                        } else {
                          vErrors.push(err8);
                        }
                        errors++;
                      }
                      var valid7 = _errs19 === errors;
                    } else {
                      var valid7 = true;
                    }
                    if (valid7) {
                      if (data0.reason !== void 0) {
                        const _errs20 = errors;
                        if ("withheld_by_privacy_policy" !== data0.reason) {
                          const err9 = { instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err9];
                          } else {
                            vErrors.push(err9);
                          }
                          errors++;
                        }
                        var valid7 = _errs20 === errors;
                      } else {
                        var valid7 = true;
                      }
                    }
                  }
                  var _valid0 = _errs18 === errors;
                  if (_valid0 && valid2) {
                    valid2 = false;
                    passing0 = [passing0, 4];
                  } else {
                    if (_valid0) {
                      valid2 = true;
                      passing0 = 4;
                      if (props0 !== true) {
                        props0 = props0 || {};
                        props0.state = true;
                        props0.reason = true;
                      }
                    }
                  }
                }
              }
            }
            if (!valid2) {
              const err10 = { instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing0 }, message: "must match exactly one schema in oneOf" };
              if (vErrors === null) {
                vErrors = [err10];
              } else {
                vErrors.push(err10);
              }
              errors++;
              validate30.errors = vErrors;
              return false;
            } else {
              errors = _errs5;
              if (vErrors !== null) {
                if (_errs5) {
                  vErrors.length = _errs5;
                } else {
                  vErrors = null;
                }
              }
            }
            if (errors === _errs3) {
              if (data0 && typeof data0 == "object" && !Array.isArray(data0)) {
                let missing1;
                if (data0.state === void 0 && (missing1 = "state") || data0.reason === void 0 && (missing1 = "reason")) {
                  validate30.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                  return false;
                } else {
                  const _errs21 = errors;
                  for (const key1 in data0) {
                    if (!(key1 === "state" || key1 === "reason")) {
                      validate30.errors = [{ instancePath: instancePath + "/repository", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                      return false;
                      break;
                    }
                  }
                  if (_errs21 === errors) {
                    if (data0.state !== void 0) {
                      let data11 = data0.state;
                      const _errs22 = errors;
                      if (!(data11 === "available" || data11 === "source_unavailable" || data11 === "withheld" || data11 === "not_applicable" || data11 === "private_lookup")) {
                        validate30.errors = [{ instancePath: instancePath + "/repository/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                        return false;
                      }
                      var valid8 = _errs22 === errors;
                    } else {
                      var valid8 = true;
                    }
                    if (valid8) {
                      if (data0.reason !== void 0) {
                        let data12 = data0.reason;
                        const _errs23 = errors;
                        if (typeof data12 !== "string") {
                          validate30.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        if (!(data12 === "reported_by_adapter" || data12 === "derived_from_trace_context" || data12 === "legacy_v1_report" || data12 === "source_not_provided" || data12 === "partial_token_metrics" || data12 === "historical_codex_source_not_lookup_eligible" || data12 === "codex_notify_turn_correlation_unavailable" || data12 === "ambiguous_trace_repository" || data12 === "span_kind_not_model_backed" || data12 === "span_kind_has_no_latency" || data12 === "span_kind_has_no_token_usage" || data12 === "claude_private_lookup_not_supported" || data12 === "cursor_private_lookup_not_supported" || data12 === "codex_span_not_notify_derived" || data12 === "agent_private_lookup_not_supported" || data12 === "local_opt_in_lookup_required" || data12 === "withheld_by_privacy_policy")) {
                          validate30.errors = [{ instancePath: instancePath + "/repository/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid8 = _errs23 === errors;
                      } else {
                        var valid8 = true;
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
              let data13 = data.turn;
              const _errs25 = errors;
              const _errs26 = errors;
              const _errs28 = errors;
              let valid10 = false;
              let passing1 = null;
              const _errs29 = errors;
              if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                if (data13.state !== void 0) {
                  const _errs30 = errors;
                  if ("available" !== data13.state) {
                    const err11 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err11];
                    } else {
                      vErrors.push(err11);
                    }
                    errors++;
                  }
                  var valid11 = _errs30 === errors;
                } else {
                  var valid11 = true;
                }
                if (valid11) {
                  if (data13.reason !== void 0) {
                    let data15 = data13.reason;
                    const _errs31 = errors;
                    if (!(data15 === "reported_by_adapter" || data15 === "derived_from_trace_context" || data15 === "legacy_v1_report")) {
                      const err12 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err12];
                      } else {
                        vErrors.push(err12);
                      }
                      errors++;
                    }
                    var valid11 = _errs31 === errors;
                  } else {
                    var valid11 = true;
                  }
                }
              }
              var _valid1 = _errs29 === errors;
              if (_valid1) {
                valid10 = true;
                passing1 = 0;
                var props1 = {};
                props1.state = true;
                props1.reason = true;
              }
              const _errs32 = errors;
              if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                if (data13.state !== void 0) {
                  const _errs33 = errors;
                  if ("source_unavailable" !== data13.state) {
                    const err13 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                    if (vErrors === null) {
                      vErrors = [err13];
                    } else {
                      vErrors.push(err13);
                    }
                    errors++;
                  }
                  var valid12 = _errs33 === errors;
                } else {
                  var valid12 = true;
                }
                if (valid12) {
                  if (data13.reason !== void 0) {
                    let data17 = data13.reason;
                    const _errs34 = errors;
                    if (!(data17 === "source_not_provided" || data17 === "partial_token_metrics" || data17 === "historical_codex_source_not_lookup_eligible" || data17 === "codex_notify_turn_correlation_unavailable" || data17 === "ambiguous_trace_repository" || data17 === "legacy_v1_report")) {
                      const err14 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                      if (vErrors === null) {
                        vErrors = [err14];
                      } else {
                        vErrors.push(err14);
                      }
                      errors++;
                    }
                    var valid12 = _errs34 === errors;
                  } else {
                    var valid12 = true;
                  }
                }
              }
              var _valid1 = _errs32 === errors;
              if (_valid1 && valid10) {
                valid10 = false;
                passing1 = [passing1, 1];
              } else {
                if (_valid1) {
                  valid10 = true;
                  passing1 = 1;
                  if (props1 !== true) {
                    props1 = props1 || {};
                    props1.state = true;
                    props1.reason = true;
                  }
                }
                const _errs35 = errors;
                if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                  if (data13.state !== void 0) {
                    const _errs36 = errors;
                    if ("not_applicable" !== data13.state) {
                      const err15 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err15];
                      } else {
                        vErrors.push(err15);
                      }
                      errors++;
                    }
                    var valid13 = _errs36 === errors;
                  } else {
                    var valid13 = true;
                  }
                  if (valid13) {
                    if (data13.reason !== void 0) {
                      let data19 = data13.reason;
                      const _errs37 = errors;
                      if (!(data19 === "span_kind_not_model_backed" || data19 === "span_kind_has_no_latency" || data19 === "span_kind_has_no_token_usage" || data19 === "claude_private_lookup_not_supported" || data19 === "cursor_private_lookup_not_supported" || data19 === "codex_span_not_notify_derived" || data19 === "agent_private_lookup_not_supported")) {
                        const err16 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err16];
                        } else {
                          vErrors.push(err16);
                        }
                        errors++;
                      }
                      var valid13 = _errs37 === errors;
                    } else {
                      var valid13 = true;
                    }
                  }
                }
                var _valid1 = _errs35 === errors;
                if (_valid1 && valid10) {
                  valid10 = false;
                  passing1 = [passing1, 2];
                } else {
                  if (_valid1) {
                    valid10 = true;
                    passing1 = 2;
                    if (props1 !== true) {
                      props1 = props1 || {};
                      props1.state = true;
                      props1.reason = true;
                    }
                  }
                  const _errs38 = errors;
                  if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                    if (data13.state !== void 0) {
                      const _errs39 = errors;
                      if ("private_lookup" !== data13.state) {
                        const err17 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err17];
                        } else {
                          vErrors.push(err17);
                        }
                        errors++;
                      }
                      var valid14 = _errs39 === errors;
                    } else {
                      var valid14 = true;
                    }
                    if (valid14) {
                      if (data13.reason !== void 0) {
                        const _errs40 = errors;
                        if ("local_opt_in_lookup_required" !== data13.reason) {
                          const err18 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err18];
                          } else {
                            vErrors.push(err18);
                          }
                          errors++;
                        }
                        var valid14 = _errs40 === errors;
                      } else {
                        var valid14 = true;
                      }
                    }
                  }
                  var _valid1 = _errs38 === errors;
                  if (_valid1 && valid10) {
                    valid10 = false;
                    passing1 = [passing1, 3];
                  } else {
                    if (_valid1) {
                      valid10 = true;
                      passing1 = 3;
                      if (props1 !== true) {
                        props1 = props1 || {};
                        props1.state = true;
                        props1.reason = true;
                      }
                    }
                    const _errs41 = errors;
                    if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                      if (data13.state !== void 0) {
                        const _errs42 = errors;
                        if ("withheld" !== data13.state) {
                          const err19 = { instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err19];
                          } else {
                            vErrors.push(err19);
                          }
                          errors++;
                        }
                        var valid15 = _errs42 === errors;
                      } else {
                        var valid15 = true;
                      }
                      if (valid15) {
                        if (data13.reason !== void 0) {
                          const _errs43 = errors;
                          if ("withheld_by_privacy_policy" !== data13.reason) {
                            const err20 = { instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err20];
                            } else {
                              vErrors.push(err20);
                            }
                            errors++;
                          }
                          var valid15 = _errs43 === errors;
                        } else {
                          var valid15 = true;
                        }
                      }
                    }
                    var _valid1 = _errs41 === errors;
                    if (_valid1 && valid10) {
                      valid10 = false;
                      passing1 = [passing1, 4];
                    } else {
                      if (_valid1) {
                        valid10 = true;
                        passing1 = 4;
                        if (props1 !== true) {
                          props1 = props1 || {};
                          props1.state = true;
                          props1.reason = true;
                        }
                      }
                    }
                  }
                }
              }
              if (!valid10) {
                const err21 = { instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing1 }, message: "must match exactly one schema in oneOf" };
                if (vErrors === null) {
                  vErrors = [err21];
                } else {
                  vErrors.push(err21);
                }
                errors++;
                validate30.errors = vErrors;
                return false;
              } else {
                errors = _errs28;
                if (vErrors !== null) {
                  if (_errs28) {
                    vErrors.length = _errs28;
                  } else {
                    vErrors = null;
                  }
                }
              }
              if (errors === _errs26) {
                if (data13 && typeof data13 == "object" && !Array.isArray(data13)) {
                  let missing2;
                  if (data13.state === void 0 && (missing2 = "state") || data13.reason === void 0 && (missing2 = "reason")) {
                    validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                    return false;
                  } else {
                    const _errs44 = errors;
                    for (const key2 in data13) {
                      if (!(key2 === "state" || key2 === "reason")) {
                        validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                        return false;
                        break;
                      }
                    }
                    if (_errs44 === errors) {
                      if (data13.state !== void 0) {
                        let data24 = data13.state;
                        const _errs45 = errors;
                        if (!(data24 === "available" || data24 === "source_unavailable" || data24 === "withheld" || data24 === "not_applicable" || data24 === "private_lookup")) {
                          validate30.errors = [{ instancePath: instancePath + "/turn/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                          return false;
                        }
                        var valid16 = _errs45 === errors;
                      } else {
                        var valid16 = true;
                      }
                      if (valid16) {
                        if (data13.reason !== void 0) {
                          let data25 = data13.reason;
                          const _errs46 = errors;
                          if (typeof data25 !== "string") {
                            validate30.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                            return false;
                          }
                          if (!(data25 === "reported_by_adapter" || data25 === "derived_from_trace_context" || data25 === "legacy_v1_report" || data25 === "source_not_provided" || data25 === "partial_token_metrics" || data25 === "historical_codex_source_not_lookup_eligible" || data25 === "codex_notify_turn_correlation_unavailable" || data25 === "ambiguous_trace_repository" || data25 === "span_kind_not_model_backed" || data25 === "span_kind_has_no_latency" || data25 === "span_kind_has_no_token_usage" || data25 === "claude_private_lookup_not_supported" || data25 === "cursor_private_lookup_not_supported" || data25 === "codex_span_not_notify_derived" || data25 === "agent_private_lookup_not_supported" || data25 === "local_opt_in_lookup_required" || data25 === "withheld_by_privacy_policy")) {
                            validate30.errors = [{ instancePath: instancePath + "/turn/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid16 = _errs46 === errors;
                        } else {
                          var valid16 = true;
                        }
                      }
                    }
                  }
                } else {
                  validate30.errors = [{ instancePath: instancePath + "/turn", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                  return false;
                }
              }
              var valid0 = _errs25 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.model !== void 0) {
                let data26 = data.model;
                const _errs48 = errors;
                const _errs49 = errors;
                const _errs51 = errors;
                let valid18 = false;
                let passing2 = null;
                const _errs52 = errors;
                if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                  if (data26.state !== void 0) {
                    const _errs53 = errors;
                    if ("available" !== data26.state) {
                      const err22 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err22];
                      } else {
                        vErrors.push(err22);
                      }
                      errors++;
                    }
                    var valid19 = _errs53 === errors;
                  } else {
                    var valid19 = true;
                  }
                  if (valid19) {
                    if (data26.reason !== void 0) {
                      let data28 = data26.reason;
                      const _errs54 = errors;
                      if (!(data28 === "reported_by_adapter" || data28 === "derived_from_trace_context" || data28 === "legacy_v1_report")) {
                        const err23 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err23];
                        } else {
                          vErrors.push(err23);
                        }
                        errors++;
                      }
                      var valid19 = _errs54 === errors;
                    } else {
                      var valid19 = true;
                    }
                  }
                }
                var _valid2 = _errs52 === errors;
                if (_valid2) {
                  valid18 = true;
                  passing2 = 0;
                  var props2 = {};
                  props2.state = true;
                  props2.reason = true;
                }
                const _errs55 = errors;
                if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                  if (data26.state !== void 0) {
                    const _errs56 = errors;
                    if ("source_unavailable" !== data26.state) {
                      const err24 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                      if (vErrors === null) {
                        vErrors = [err24];
                      } else {
                        vErrors.push(err24);
                      }
                      errors++;
                    }
                    var valid20 = _errs56 === errors;
                  } else {
                    var valid20 = true;
                  }
                  if (valid20) {
                    if (data26.reason !== void 0) {
                      let data30 = data26.reason;
                      const _errs57 = errors;
                      if (!(data30 === "source_not_provided" || data30 === "partial_token_metrics" || data30 === "historical_codex_source_not_lookup_eligible" || data30 === "codex_notify_turn_correlation_unavailable" || data30 === "ambiguous_trace_repository" || data30 === "legacy_v1_report")) {
                        const err25 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                        if (vErrors === null) {
                          vErrors = [err25];
                        } else {
                          vErrors.push(err25);
                        }
                        errors++;
                      }
                      var valid20 = _errs57 === errors;
                    } else {
                      var valid20 = true;
                    }
                  }
                }
                var _valid2 = _errs55 === errors;
                if (_valid2 && valid18) {
                  valid18 = false;
                  passing2 = [passing2, 1];
                } else {
                  if (_valid2) {
                    valid18 = true;
                    passing2 = 1;
                    if (props2 !== true) {
                      props2 = props2 || {};
                      props2.state = true;
                      props2.reason = true;
                    }
                  }
                  const _errs58 = errors;
                  if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                    if (data26.state !== void 0) {
                      const _errs59 = errors;
                      if ("not_applicable" !== data26.state) {
                        const err26 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err26];
                        } else {
                          vErrors.push(err26);
                        }
                        errors++;
                      }
                      var valid21 = _errs59 === errors;
                    } else {
                      var valid21 = true;
                    }
                    if (valid21) {
                      if (data26.reason !== void 0) {
                        let data32 = data26.reason;
                        const _errs60 = errors;
                        if (!(data32 === "span_kind_not_model_backed" || data32 === "span_kind_has_no_latency" || data32 === "span_kind_has_no_token_usage" || data32 === "claude_private_lookup_not_supported" || data32 === "cursor_private_lookup_not_supported" || data32 === "codex_span_not_notify_derived" || data32 === "agent_private_lookup_not_supported")) {
                          const err27 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err27];
                          } else {
                            vErrors.push(err27);
                          }
                          errors++;
                        }
                        var valid21 = _errs60 === errors;
                      } else {
                        var valid21 = true;
                      }
                    }
                  }
                  var _valid2 = _errs58 === errors;
                  if (_valid2 && valid18) {
                    valid18 = false;
                    passing2 = [passing2, 2];
                  } else {
                    if (_valid2) {
                      valid18 = true;
                      passing2 = 2;
                      if (props2 !== true) {
                        props2 = props2 || {};
                        props2.state = true;
                        props2.reason = true;
                      }
                    }
                    const _errs61 = errors;
                    if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                      if (data26.state !== void 0) {
                        const _errs62 = errors;
                        if ("private_lookup" !== data26.state) {
                          const err28 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err28];
                          } else {
                            vErrors.push(err28);
                          }
                          errors++;
                        }
                        var valid22 = _errs62 === errors;
                      } else {
                        var valid22 = true;
                      }
                      if (valid22) {
                        if (data26.reason !== void 0) {
                          const _errs63 = errors;
                          if ("local_opt_in_lookup_required" !== data26.reason) {
                            const err29 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err29];
                            } else {
                              vErrors.push(err29);
                            }
                            errors++;
                          }
                          var valid22 = _errs63 === errors;
                        } else {
                          var valid22 = true;
                        }
                      }
                    }
                    var _valid2 = _errs61 === errors;
                    if (_valid2 && valid18) {
                      valid18 = false;
                      passing2 = [passing2, 3];
                    } else {
                      if (_valid2) {
                        valid18 = true;
                        passing2 = 3;
                        if (props2 !== true) {
                          props2 = props2 || {};
                          props2.state = true;
                          props2.reason = true;
                        }
                      }
                      const _errs64 = errors;
                      if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                        if (data26.state !== void 0) {
                          const _errs65 = errors;
                          if ("withheld" !== data26.state) {
                            const err30 = { instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err30];
                            } else {
                              vErrors.push(err30);
                            }
                            errors++;
                          }
                          var valid23 = _errs65 === errors;
                        } else {
                          var valid23 = true;
                        }
                        if (valid23) {
                          if (data26.reason !== void 0) {
                            const _errs66 = errors;
                            if ("withheld_by_privacy_policy" !== data26.reason) {
                              const err31 = { instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err31];
                              } else {
                                vErrors.push(err31);
                              }
                              errors++;
                            }
                            var valid23 = _errs66 === errors;
                          } else {
                            var valid23 = true;
                          }
                        }
                      }
                      var _valid2 = _errs64 === errors;
                      if (_valid2 && valid18) {
                        valid18 = false;
                        passing2 = [passing2, 4];
                      } else {
                        if (_valid2) {
                          valid18 = true;
                          passing2 = 4;
                          if (props2 !== true) {
                            props2 = props2 || {};
                            props2.state = true;
                            props2.reason = true;
                          }
                        }
                      }
                    }
                  }
                }
                if (!valid18) {
                  const err32 = { instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing2 }, message: "must match exactly one schema in oneOf" };
                  if (vErrors === null) {
                    vErrors = [err32];
                  } else {
                    vErrors.push(err32);
                  }
                  errors++;
                  validate30.errors = vErrors;
                  return false;
                } else {
                  errors = _errs51;
                  if (vErrors !== null) {
                    if (_errs51) {
                      vErrors.length = _errs51;
                    } else {
                      vErrors = null;
                    }
                  }
                }
                if (errors === _errs49) {
                  if (data26 && typeof data26 == "object" && !Array.isArray(data26)) {
                    let missing3;
                    if (data26.state === void 0 && (missing3 = "state") || data26.reason === void 0 && (missing3 = "reason")) {
                      validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" }];
                      return false;
                    } else {
                      const _errs67 = errors;
                      for (const key3 in data26) {
                        if (!(key3 === "state" || key3 === "reason")) {
                          validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" }];
                          return false;
                          break;
                        }
                      }
                      if (_errs67 === errors) {
                        if (data26.state !== void 0) {
                          let data37 = data26.state;
                          const _errs68 = errors;
                          if (!(data37 === "available" || data37 === "source_unavailable" || data37 === "withheld" || data37 === "not_applicable" || data37 === "private_lookup")) {
                            validate30.errors = [{ instancePath: instancePath + "/model/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                            return false;
                          }
                          var valid24 = _errs68 === errors;
                        } else {
                          var valid24 = true;
                        }
                        if (valid24) {
                          if (data26.reason !== void 0) {
                            let data38 = data26.reason;
                            const _errs69 = errors;
                            if (typeof data38 !== "string") {
                              validate30.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                              return false;
                            }
                            if (!(data38 === "reported_by_adapter" || data38 === "derived_from_trace_context" || data38 === "legacy_v1_report" || data38 === "source_not_provided" || data38 === "partial_token_metrics" || data38 === "historical_codex_source_not_lookup_eligible" || data38 === "codex_notify_turn_correlation_unavailable" || data38 === "ambiguous_trace_repository" || data38 === "span_kind_not_model_backed" || data38 === "span_kind_has_no_latency" || data38 === "span_kind_has_no_token_usage" || data38 === "claude_private_lookup_not_supported" || data38 === "cursor_private_lookup_not_supported" || data38 === "codex_span_not_notify_derived" || data38 === "agent_private_lookup_not_supported" || data38 === "local_opt_in_lookup_required" || data38 === "withheld_by_privacy_policy")) {
                              validate30.errors = [{ instancePath: instancePath + "/model/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid24 = _errs69 === errors;
                          } else {
                            var valid24 = true;
                          }
                        }
                      }
                    }
                  } else {
                    validate30.errors = [{ instancePath: instancePath + "/model", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                    return false;
                  }
                }
                var valid0 = _errs48 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.tokens !== void 0) {
                  let data39 = data.tokens;
                  const _errs71 = errors;
                  const _errs72 = errors;
                  const _errs74 = errors;
                  let valid26 = false;
                  let passing3 = null;
                  const _errs75 = errors;
                  if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                    if (data39.state !== void 0) {
                      const _errs76 = errors;
                      if ("available" !== data39.state) {
                        const err33 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err33];
                        } else {
                          vErrors.push(err33);
                        }
                        errors++;
                      }
                      var valid27 = _errs76 === errors;
                    } else {
                      var valid27 = true;
                    }
                    if (valid27) {
                      if (data39.reason !== void 0) {
                        let data41 = data39.reason;
                        const _errs77 = errors;
                        if (!(data41 === "reported_by_adapter" || data41 === "derived_from_trace_context" || data41 === "legacy_v1_report")) {
                          const err34 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err34];
                          } else {
                            vErrors.push(err34);
                          }
                          errors++;
                        }
                        var valid27 = _errs77 === errors;
                      } else {
                        var valid27 = true;
                      }
                    }
                  }
                  var _valid3 = _errs75 === errors;
                  if (_valid3) {
                    valid26 = true;
                    passing3 = 0;
                    var props3 = {};
                    props3.state = true;
                    props3.reason = true;
                  }
                  const _errs78 = errors;
                  if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                    if (data39.state !== void 0) {
                      const _errs79 = errors;
                      if ("source_unavailable" !== data39.state) {
                        const err35 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                        if (vErrors === null) {
                          vErrors = [err35];
                        } else {
                          vErrors.push(err35);
                        }
                        errors++;
                      }
                      var valid28 = _errs79 === errors;
                    } else {
                      var valid28 = true;
                    }
                    if (valid28) {
                      if (data39.reason !== void 0) {
                        let data43 = data39.reason;
                        const _errs80 = errors;
                        if (!(data43 === "source_not_provided" || data43 === "partial_token_metrics" || data43 === "historical_codex_source_not_lookup_eligible" || data43 === "codex_notify_turn_correlation_unavailable" || data43 === "ambiguous_trace_repository" || data43 === "legacy_v1_report")) {
                          const err36 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                          if (vErrors === null) {
                            vErrors = [err36];
                          } else {
                            vErrors.push(err36);
                          }
                          errors++;
                        }
                        var valid28 = _errs80 === errors;
                      } else {
                        var valid28 = true;
                      }
                    }
                  }
                  var _valid3 = _errs78 === errors;
                  if (_valid3 && valid26) {
                    valid26 = false;
                    passing3 = [passing3, 1];
                  } else {
                    if (_valid3) {
                      valid26 = true;
                      passing3 = 1;
                      if (props3 !== true) {
                        props3 = props3 || {};
                        props3.state = true;
                        props3.reason = true;
                      }
                    }
                    const _errs81 = errors;
                    if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                      if (data39.state !== void 0) {
                        const _errs82 = errors;
                        if ("not_applicable" !== data39.state) {
                          const err37 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err37];
                          } else {
                            vErrors.push(err37);
                          }
                          errors++;
                        }
                        var valid29 = _errs82 === errors;
                      } else {
                        var valid29 = true;
                      }
                      if (valid29) {
                        if (data39.reason !== void 0) {
                          let data45 = data39.reason;
                          const _errs83 = errors;
                          if (!(data45 === "span_kind_not_model_backed" || data45 === "span_kind_has_no_latency" || data45 === "span_kind_has_no_token_usage" || data45 === "claude_private_lookup_not_supported" || data45 === "cursor_private_lookup_not_supported" || data45 === "codex_span_not_notify_derived" || data45 === "agent_private_lookup_not_supported")) {
                            const err38 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err38];
                            } else {
                              vErrors.push(err38);
                            }
                            errors++;
                          }
                          var valid29 = _errs83 === errors;
                        } else {
                          var valid29 = true;
                        }
                      }
                    }
                    var _valid3 = _errs81 === errors;
                    if (_valid3 && valid26) {
                      valid26 = false;
                      passing3 = [passing3, 2];
                    } else {
                      if (_valid3) {
                        valid26 = true;
                        passing3 = 2;
                        if (props3 !== true) {
                          props3 = props3 || {};
                          props3.state = true;
                          props3.reason = true;
                        }
                      }
                      const _errs84 = errors;
                      if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                        if (data39.state !== void 0) {
                          const _errs85 = errors;
                          if ("private_lookup" !== data39.state) {
                            const err39 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err39];
                            } else {
                              vErrors.push(err39);
                            }
                            errors++;
                          }
                          var valid30 = _errs85 === errors;
                        } else {
                          var valid30 = true;
                        }
                        if (valid30) {
                          if (data39.reason !== void 0) {
                            const _errs86 = errors;
                            if ("local_opt_in_lookup_required" !== data39.reason) {
                              const err40 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err40];
                              } else {
                                vErrors.push(err40);
                              }
                              errors++;
                            }
                            var valid30 = _errs86 === errors;
                          } else {
                            var valid30 = true;
                          }
                        }
                      }
                      var _valid3 = _errs84 === errors;
                      if (_valid3 && valid26) {
                        valid26 = false;
                        passing3 = [passing3, 3];
                      } else {
                        if (_valid3) {
                          valid26 = true;
                          passing3 = 3;
                          if (props3 !== true) {
                            props3 = props3 || {};
                            props3.state = true;
                            props3.reason = true;
                          }
                        }
                        const _errs87 = errors;
                        if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                          if (data39.state !== void 0) {
                            const _errs88 = errors;
                            if ("withheld" !== data39.state) {
                              const err41 = { instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err41];
                              } else {
                                vErrors.push(err41);
                              }
                              errors++;
                            }
                            var valid31 = _errs88 === errors;
                          } else {
                            var valid31 = true;
                          }
                          if (valid31) {
                            if (data39.reason !== void 0) {
                              const _errs89 = errors;
                              if ("withheld_by_privacy_policy" !== data39.reason) {
                                const err42 = { instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err42];
                                } else {
                                  vErrors.push(err42);
                                }
                                errors++;
                              }
                              var valid31 = _errs89 === errors;
                            } else {
                              var valid31 = true;
                            }
                          }
                        }
                        var _valid3 = _errs87 === errors;
                        if (_valid3 && valid26) {
                          valid26 = false;
                          passing3 = [passing3, 4];
                        } else {
                          if (_valid3) {
                            valid26 = true;
                            passing3 = 4;
                            if (props3 !== true) {
                              props3 = props3 || {};
                              props3.state = true;
                              props3.reason = true;
                            }
                          }
                        }
                      }
                    }
                  }
                  if (!valid26) {
                    const err43 = { instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing3 }, message: "must match exactly one schema in oneOf" };
                    if (vErrors === null) {
                      vErrors = [err43];
                    } else {
                      vErrors.push(err43);
                    }
                    errors++;
                    validate30.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs74;
                    if (vErrors !== null) {
                      if (_errs74) {
                        vErrors.length = _errs74;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  if (errors === _errs72) {
                    if (data39 && typeof data39 == "object" && !Array.isArray(data39)) {
                      let missing4;
                      if (data39.state === void 0 && (missing4 = "state") || data39.reason === void 0 && (missing4 = "reason")) {
                        validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
                        return false;
                      } else {
                        const _errs90 = errors;
                        for (const key4 in data39) {
                          if (!(key4 === "state" || key4 === "reason")) {
                            validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" }];
                            return false;
                            break;
                          }
                        }
                        if (_errs90 === errors) {
                          if (data39.state !== void 0) {
                            let data50 = data39.state;
                            const _errs91 = errors;
                            if (!(data50 === "available" || data50 === "source_unavailable" || data50 === "withheld" || data50 === "not_applicable" || data50 === "private_lookup")) {
                              validate30.errors = [{ instancePath: instancePath + "/tokens/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                              return false;
                            }
                            var valid32 = _errs91 === errors;
                          } else {
                            var valid32 = true;
                          }
                          if (valid32) {
                            if (data39.reason !== void 0) {
                              let data51 = data39.reason;
                              const _errs92 = errors;
                              if (typeof data51 !== "string") {
                                validate30.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                return false;
                              }
                              if (!(data51 === "reported_by_adapter" || data51 === "derived_from_trace_context" || data51 === "legacy_v1_report" || data51 === "source_not_provided" || data51 === "partial_token_metrics" || data51 === "historical_codex_source_not_lookup_eligible" || data51 === "codex_notify_turn_correlation_unavailable" || data51 === "ambiguous_trace_repository" || data51 === "span_kind_not_model_backed" || data51 === "span_kind_has_no_latency" || data51 === "span_kind_has_no_token_usage" || data51 === "claude_private_lookup_not_supported" || data51 === "cursor_private_lookup_not_supported" || data51 === "codex_span_not_notify_derived" || data51 === "agent_private_lookup_not_supported" || data51 === "local_opt_in_lookup_required" || data51 === "withheld_by_privacy_policy")) {
                                validate30.errors = [{ instancePath: instancePath + "/tokens/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                return false;
                              }
                              var valid32 = _errs92 === errors;
                            } else {
                              var valid32 = true;
                            }
                          }
                        }
                      }
                    } else {
                      validate30.errors = [{ instancePath: instancePath + "/tokens", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                      return false;
                    }
                  }
                  var valid0 = _errs71 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.latency !== void 0) {
                    let data52 = data.latency;
                    const _errs94 = errors;
                    const _errs95 = errors;
                    const _errs97 = errors;
                    let valid34 = false;
                    let passing4 = null;
                    const _errs98 = errors;
                    if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                      if (data52.state !== void 0) {
                        const _errs99 = errors;
                        if ("available" !== data52.state) {
                          const err44 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err44];
                          } else {
                            vErrors.push(err44);
                          }
                          errors++;
                        }
                        var valid35 = _errs99 === errors;
                      } else {
                        var valid35 = true;
                      }
                      if (valid35) {
                        if (data52.reason !== void 0) {
                          let data54 = data52.reason;
                          const _errs100 = errors;
                          if (!(data54 === "reported_by_adapter" || data54 === "derived_from_trace_context" || data54 === "legacy_v1_report")) {
                            const err45 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err45];
                            } else {
                              vErrors.push(err45);
                            }
                            errors++;
                          }
                          var valid35 = _errs100 === errors;
                        } else {
                          var valid35 = true;
                        }
                      }
                    }
                    var _valid4 = _errs98 === errors;
                    if (_valid4) {
                      valid34 = true;
                      passing4 = 0;
                      var props4 = {};
                      props4.state = true;
                      props4.reason = true;
                    }
                    const _errs101 = errors;
                    if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                      if (data52.state !== void 0) {
                        const _errs102 = errors;
                        if ("source_unavailable" !== data52.state) {
                          const err46 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                          if (vErrors === null) {
                            vErrors = [err46];
                          } else {
                            vErrors.push(err46);
                          }
                          errors++;
                        }
                        var valid36 = _errs102 === errors;
                      } else {
                        var valid36 = true;
                      }
                      if (valid36) {
                        if (data52.reason !== void 0) {
                          let data56 = data52.reason;
                          const _errs103 = errors;
                          if (!(data56 === "source_not_provided" || data56 === "partial_token_metrics" || data56 === "historical_codex_source_not_lookup_eligible" || data56 === "codex_notify_turn_correlation_unavailable" || data56 === "ambiguous_trace_repository" || data56 === "legacy_v1_report")) {
                            const err47 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                            if (vErrors === null) {
                              vErrors = [err47];
                            } else {
                              vErrors.push(err47);
                            }
                            errors++;
                          }
                          var valid36 = _errs103 === errors;
                        } else {
                          var valid36 = true;
                        }
                      }
                    }
                    var _valid4 = _errs101 === errors;
                    if (_valid4 && valid34) {
                      valid34 = false;
                      passing4 = [passing4, 1];
                    } else {
                      if (_valid4) {
                        valid34 = true;
                        passing4 = 1;
                        if (props4 !== true) {
                          props4 = props4 || {};
                          props4.state = true;
                          props4.reason = true;
                        }
                      }
                      const _errs104 = errors;
                      if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                        if (data52.state !== void 0) {
                          const _errs105 = errors;
                          if ("not_applicable" !== data52.state) {
                            const err48 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err48];
                            } else {
                              vErrors.push(err48);
                            }
                            errors++;
                          }
                          var valid37 = _errs105 === errors;
                        } else {
                          var valid37 = true;
                        }
                        if (valid37) {
                          if (data52.reason !== void 0) {
                            let data58 = data52.reason;
                            const _errs106 = errors;
                            if (!(data58 === "span_kind_not_model_backed" || data58 === "span_kind_has_no_latency" || data58 === "span_kind_has_no_token_usage" || data58 === "claude_private_lookup_not_supported" || data58 === "cursor_private_lookup_not_supported" || data58 === "codex_span_not_notify_derived" || data58 === "agent_private_lookup_not_supported")) {
                              const err49 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err49];
                              } else {
                                vErrors.push(err49);
                              }
                              errors++;
                            }
                            var valid37 = _errs106 === errors;
                          } else {
                            var valid37 = true;
                          }
                        }
                      }
                      var _valid4 = _errs104 === errors;
                      if (_valid4 && valid34) {
                        valid34 = false;
                        passing4 = [passing4, 2];
                      } else {
                        if (_valid4) {
                          valid34 = true;
                          passing4 = 2;
                          if (props4 !== true) {
                            props4 = props4 || {};
                            props4.state = true;
                            props4.reason = true;
                          }
                        }
                        const _errs107 = errors;
                        if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                          if (data52.state !== void 0) {
                            const _errs108 = errors;
                            if ("private_lookup" !== data52.state) {
                              const err50 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err50];
                              } else {
                                vErrors.push(err50);
                              }
                              errors++;
                            }
                            var valid38 = _errs108 === errors;
                          } else {
                            var valid38 = true;
                          }
                          if (valid38) {
                            if (data52.reason !== void 0) {
                              const _errs109 = errors;
                              if ("local_opt_in_lookup_required" !== data52.reason) {
                                const err51 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err51];
                                } else {
                                  vErrors.push(err51);
                                }
                                errors++;
                              }
                              var valid38 = _errs109 === errors;
                            } else {
                              var valid38 = true;
                            }
                          }
                        }
                        var _valid4 = _errs107 === errors;
                        if (_valid4 && valid34) {
                          valid34 = false;
                          passing4 = [passing4, 3];
                        } else {
                          if (_valid4) {
                            valid34 = true;
                            passing4 = 3;
                            if (props4 !== true) {
                              props4 = props4 || {};
                              props4.state = true;
                              props4.reason = true;
                            }
                          }
                          const _errs110 = errors;
                          if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                            if (data52.state !== void 0) {
                              const _errs111 = errors;
                              if ("withheld" !== data52.state) {
                                const err52 = { instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err52];
                                } else {
                                  vErrors.push(err52);
                                }
                                errors++;
                              }
                              var valid39 = _errs111 === errors;
                            } else {
                              var valid39 = true;
                            }
                            if (valid39) {
                              if (data52.reason !== void 0) {
                                const _errs112 = errors;
                                if ("withheld_by_privacy_policy" !== data52.reason) {
                                  const err53 = { instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err53];
                                  } else {
                                    vErrors.push(err53);
                                  }
                                  errors++;
                                }
                                var valid39 = _errs112 === errors;
                              } else {
                                var valid39 = true;
                              }
                            }
                          }
                          var _valid4 = _errs110 === errors;
                          if (_valid4 && valid34) {
                            valid34 = false;
                            passing4 = [passing4, 4];
                          } else {
                            if (_valid4) {
                              valid34 = true;
                              passing4 = 4;
                              if (props4 !== true) {
                                props4 = props4 || {};
                                props4.state = true;
                                props4.reason = true;
                              }
                            }
                          }
                        }
                      }
                    }
                    if (!valid34) {
                      const err54 = { instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing4 }, message: "must match exactly one schema in oneOf" };
                      if (vErrors === null) {
                        vErrors = [err54];
                      } else {
                        vErrors.push(err54);
                      }
                      errors++;
                      validate30.errors = vErrors;
                      return false;
                    } else {
                      errors = _errs97;
                      if (vErrors !== null) {
                        if (_errs97) {
                          vErrors.length = _errs97;
                        } else {
                          vErrors = null;
                        }
                      }
                    }
                    if (errors === _errs95) {
                      if (data52 && typeof data52 == "object" && !Array.isArray(data52)) {
                        let missing5;
                        if (data52.state === void 0 && (missing5 = "state") || data52.reason === void 0 && (missing5 = "reason")) {
                          validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing5 }, message: "must have required property '" + missing5 + "'" }];
                          return false;
                        } else {
                          const _errs113 = errors;
                          for (const key5 in data52) {
                            if (!(key5 === "state" || key5 === "reason")) {
                              validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key5 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs113 === errors) {
                            if (data52.state !== void 0) {
                              let data63 = data52.state;
                              const _errs114 = errors;
                              if (!(data63 === "available" || data63 === "source_unavailable" || data63 === "withheld" || data63 === "not_applicable" || data63 === "private_lookup")) {
                                validate30.errors = [{ instancePath: instancePath + "/latency/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                return false;
                              }
                              var valid40 = _errs114 === errors;
                            } else {
                              var valid40 = true;
                            }
                            if (valid40) {
                              if (data52.reason !== void 0) {
                                let data64 = data52.reason;
                                const _errs115 = errors;
                                if (typeof data64 !== "string") {
                                  validate30.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                if (!(data64 === "reported_by_adapter" || data64 === "derived_from_trace_context" || data64 === "legacy_v1_report" || data64 === "source_not_provided" || data64 === "partial_token_metrics" || data64 === "historical_codex_source_not_lookup_eligible" || data64 === "codex_notify_turn_correlation_unavailable" || data64 === "ambiguous_trace_repository" || data64 === "span_kind_not_model_backed" || data64 === "span_kind_has_no_latency" || data64 === "span_kind_has_no_token_usage" || data64 === "claude_private_lookup_not_supported" || data64 === "cursor_private_lookup_not_supported" || data64 === "codex_span_not_notify_derived" || data64 === "agent_private_lookup_not_supported" || data64 === "local_opt_in_lookup_required" || data64 === "withheld_by_privacy_policy")) {
                                  validate30.errors = [{ instancePath: instancePath + "/latency/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid40 = _errs115 === errors;
                              } else {
                                var valid40 = true;
                              }
                            }
                          }
                        }
                      } else {
                        validate30.errors = [{ instancePath: instancePath + "/latency", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                        return false;
                      }
                    }
                    var valid0 = _errs94 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.sourceLocation !== void 0) {
                      let data65 = data.sourceLocation;
                      const _errs117 = errors;
                      const _errs118 = errors;
                      const _errs120 = errors;
                      let valid42 = false;
                      let passing5 = null;
                      const _errs121 = errors;
                      if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                        if (data65.state !== void 0) {
                          const _errs122 = errors;
                          if ("available" !== data65.state) {
                            const err55 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err55];
                            } else {
                              vErrors.push(err55);
                            }
                            errors++;
                          }
                          var valid43 = _errs122 === errors;
                        } else {
                          var valid43 = true;
                        }
                        if (valid43) {
                          if (data65.reason !== void 0) {
                            let data67 = data65.reason;
                            const _errs123 = errors;
                            if (!(data67 === "reported_by_adapter" || data67 === "derived_from_trace_context" || data67 === "legacy_v1_report")) {
                              const err56 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err56];
                              } else {
                                vErrors.push(err56);
                              }
                              errors++;
                            }
                            var valid43 = _errs123 === errors;
                          } else {
                            var valid43 = true;
                          }
                        }
                      }
                      var _valid5 = _errs121 === errors;
                      if (_valid5) {
                        valid42 = true;
                        passing5 = 0;
                        var props5 = {};
                        props5.state = true;
                        props5.reason = true;
                      }
                      const _errs124 = errors;
                      if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                        if (data65.state !== void 0) {
                          const _errs125 = errors;
                          if ("source_unavailable" !== data65.state) {
                            const err57 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                            if (vErrors === null) {
                              vErrors = [err57];
                            } else {
                              vErrors.push(err57);
                            }
                            errors++;
                          }
                          var valid44 = _errs125 === errors;
                        } else {
                          var valid44 = true;
                        }
                        if (valid44) {
                          if (data65.reason !== void 0) {
                            let data69 = data65.reason;
                            const _errs126 = errors;
                            if (!(data69 === "source_not_provided" || data69 === "partial_token_metrics" || data69 === "historical_codex_source_not_lookup_eligible" || data69 === "codex_notify_turn_correlation_unavailable" || data69 === "ambiguous_trace_repository" || data69 === "legacy_v1_report")) {
                              const err58 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                              if (vErrors === null) {
                                vErrors = [err58];
                              } else {
                                vErrors.push(err58);
                              }
                              errors++;
                            }
                            var valid44 = _errs126 === errors;
                          } else {
                            var valid44 = true;
                          }
                        }
                      }
                      var _valid5 = _errs124 === errors;
                      if (_valid5 && valid42) {
                        valid42 = false;
                        passing5 = [passing5, 1];
                      } else {
                        if (_valid5) {
                          valid42 = true;
                          passing5 = 1;
                          if (props5 !== true) {
                            props5 = props5 || {};
                            props5.state = true;
                            props5.reason = true;
                          }
                        }
                        const _errs127 = errors;
                        if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                          if (data65.state !== void 0) {
                            const _errs128 = errors;
                            if ("not_applicable" !== data65.state) {
                              const err59 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err59];
                              } else {
                                vErrors.push(err59);
                              }
                              errors++;
                            }
                            var valid45 = _errs128 === errors;
                          } else {
                            var valid45 = true;
                          }
                          if (valid45) {
                            if (data65.reason !== void 0) {
                              let data71 = data65.reason;
                              const _errs129 = errors;
                              if (!(data71 === "span_kind_not_model_backed" || data71 === "span_kind_has_no_latency" || data71 === "span_kind_has_no_token_usage" || data71 === "claude_private_lookup_not_supported" || data71 === "cursor_private_lookup_not_supported" || data71 === "codex_span_not_notify_derived" || data71 === "agent_private_lookup_not_supported")) {
                                const err60 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err60];
                                } else {
                                  vErrors.push(err60);
                                }
                                errors++;
                              }
                              var valid45 = _errs129 === errors;
                            } else {
                              var valid45 = true;
                            }
                          }
                        }
                        var _valid5 = _errs127 === errors;
                        if (_valid5 && valid42) {
                          valid42 = false;
                          passing5 = [passing5, 2];
                        } else {
                          if (_valid5) {
                            valid42 = true;
                            passing5 = 2;
                            if (props5 !== true) {
                              props5 = props5 || {};
                              props5.state = true;
                              props5.reason = true;
                            }
                          }
                          const _errs130 = errors;
                          if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                            if (data65.state !== void 0) {
                              const _errs131 = errors;
                              if ("private_lookup" !== data65.state) {
                                const err61 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err61];
                                } else {
                                  vErrors.push(err61);
                                }
                                errors++;
                              }
                              var valid46 = _errs131 === errors;
                            } else {
                              var valid46 = true;
                            }
                            if (valid46) {
                              if (data65.reason !== void 0) {
                                const _errs132 = errors;
                                if ("local_opt_in_lookup_required" !== data65.reason) {
                                  const err62 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err62];
                                  } else {
                                    vErrors.push(err62);
                                  }
                                  errors++;
                                }
                                var valid46 = _errs132 === errors;
                              } else {
                                var valid46 = true;
                              }
                            }
                          }
                          var _valid5 = _errs130 === errors;
                          if (_valid5 && valid42) {
                            valid42 = false;
                            passing5 = [passing5, 3];
                          } else {
                            if (_valid5) {
                              valid42 = true;
                              passing5 = 3;
                              if (props5 !== true) {
                                props5 = props5 || {};
                                props5.state = true;
                                props5.reason = true;
                              }
                            }
                            const _errs133 = errors;
                            if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                              if (data65.state !== void 0) {
                                const _errs134 = errors;
                                if ("withheld" !== data65.state) {
                                  const err63 = { instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err63];
                                  } else {
                                    vErrors.push(err63);
                                  }
                                  errors++;
                                }
                                var valid47 = _errs134 === errors;
                              } else {
                                var valid47 = true;
                              }
                              if (valid47) {
                                if (data65.reason !== void 0) {
                                  const _errs135 = errors;
                                  if ("withheld_by_privacy_policy" !== data65.reason) {
                                    const err64 = { instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err64];
                                    } else {
                                      vErrors.push(err64);
                                    }
                                    errors++;
                                  }
                                  var valid47 = _errs135 === errors;
                                } else {
                                  var valid47 = true;
                                }
                              }
                            }
                            var _valid5 = _errs133 === errors;
                            if (_valid5 && valid42) {
                              valid42 = false;
                              passing5 = [passing5, 4];
                            } else {
                              if (_valid5) {
                                valid42 = true;
                                passing5 = 4;
                                if (props5 !== true) {
                                  props5 = props5 || {};
                                  props5.state = true;
                                  props5.reason = true;
                                }
                              }
                            }
                          }
                        }
                      }
                      if (!valid42) {
                        const err65 = { instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing5 }, message: "must match exactly one schema in oneOf" };
                        if (vErrors === null) {
                          vErrors = [err65];
                        } else {
                          vErrors.push(err65);
                        }
                        errors++;
                        validate30.errors = vErrors;
                        return false;
                      } else {
                        errors = _errs120;
                        if (vErrors !== null) {
                          if (_errs120) {
                            vErrors.length = _errs120;
                          } else {
                            vErrors = null;
                          }
                        }
                      }
                      if (errors === _errs118) {
                        if (data65 && typeof data65 == "object" && !Array.isArray(data65)) {
                          let missing6;
                          if (data65.state === void 0 && (missing6 = "state") || data65.reason === void 0 && (missing6 = "reason")) {
                            validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing6 }, message: "must have required property '" + missing6 + "'" }];
                            return false;
                          } else {
                            const _errs136 = errors;
                            for (const key6 in data65) {
                              if (!(key6 === "state" || key6 === "reason")) {
                                validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key6 }, message: "must NOT have additional properties" }];
                                return false;
                                break;
                              }
                            }
                            if (_errs136 === errors) {
                              if (data65.state !== void 0) {
                                let data76 = data65.state;
                                const _errs137 = errors;
                                if (!(data76 === "available" || data76 === "source_unavailable" || data76 === "withheld" || data76 === "not_applicable" || data76 === "private_lookup")) {
                                  validate30.errors = [{ instancePath: instancePath + "/sourceLocation/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid48 = _errs137 === errors;
                              } else {
                                var valid48 = true;
                              }
                              if (valid48) {
                                if (data65.reason !== void 0) {
                                  let data77 = data65.reason;
                                  const _errs138 = errors;
                                  if (typeof data77 !== "string") {
                                    validate30.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                    return false;
                                  }
                                  if (!(data77 === "reported_by_adapter" || data77 === "derived_from_trace_context" || data77 === "legacy_v1_report" || data77 === "source_not_provided" || data77 === "partial_token_metrics" || data77 === "historical_codex_source_not_lookup_eligible" || data77 === "codex_notify_turn_correlation_unavailable" || data77 === "ambiguous_trace_repository" || data77 === "span_kind_not_model_backed" || data77 === "span_kind_has_no_latency" || data77 === "span_kind_has_no_token_usage" || data77 === "claude_private_lookup_not_supported" || data77 === "cursor_private_lookup_not_supported" || data77 === "codex_span_not_notify_derived" || data77 === "agent_private_lookup_not_supported" || data77 === "local_opt_in_lookup_required" || data77 === "withheld_by_privacy_policy")) {
                                    validate30.errors = [{ instancePath: instancePath + "/sourceLocation/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                    return false;
                                  }
                                  var valid48 = _errs138 === errors;
                                } else {
                                  var valid48 = true;
                                }
                              }
                            }
                          }
                        } else {
                          validate30.errors = [{ instancePath: instancePath + "/sourceLocation", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs117 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.requestContent !== void 0) {
                        let data78 = data.requestContent;
                        const _errs140 = errors;
                        const _errs141 = errors;
                        const _errs143 = errors;
                        let valid50 = false;
                        let passing6 = null;
                        const _errs144 = errors;
                        if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                          if (data78.state !== void 0) {
                            const _errs145 = errors;
                            if ("available" !== data78.state) {
                              const err66 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err66];
                              } else {
                                vErrors.push(err66);
                              }
                              errors++;
                            }
                            var valid51 = _errs145 === errors;
                          } else {
                            var valid51 = true;
                          }
                          if (valid51) {
                            if (data78.reason !== void 0) {
                              let data80 = data78.reason;
                              const _errs146 = errors;
                              if (!(data80 === "reported_by_adapter" || data80 === "derived_from_trace_context" || data80 === "legacy_v1_report")) {
                                const err67 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err67];
                                } else {
                                  vErrors.push(err67);
                                }
                                errors++;
                              }
                              var valid51 = _errs146 === errors;
                            } else {
                              var valid51 = true;
                            }
                          }
                        }
                        var _valid6 = _errs144 === errors;
                        if (_valid6) {
                          valid50 = true;
                          passing6 = 0;
                          var props6 = {};
                          props6.state = true;
                          props6.reason = true;
                        }
                        const _errs147 = errors;
                        if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                          if (data78.state !== void 0) {
                            const _errs148 = errors;
                            if ("source_unavailable" !== data78.state) {
                              const err68 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                              if (vErrors === null) {
                                vErrors = [err68];
                              } else {
                                vErrors.push(err68);
                              }
                              errors++;
                            }
                            var valid52 = _errs148 === errors;
                          } else {
                            var valid52 = true;
                          }
                          if (valid52) {
                            if (data78.reason !== void 0) {
                              let data82 = data78.reason;
                              const _errs149 = errors;
                              if (!(data82 === "source_not_provided" || data82 === "partial_token_metrics" || data82 === "historical_codex_source_not_lookup_eligible" || data82 === "codex_notify_turn_correlation_unavailable" || data82 === "ambiguous_trace_repository" || data82 === "legacy_v1_report")) {
                                const err69 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                if (vErrors === null) {
                                  vErrors = [err69];
                                } else {
                                  vErrors.push(err69);
                                }
                                errors++;
                              }
                              var valid52 = _errs149 === errors;
                            } else {
                              var valid52 = true;
                            }
                          }
                        }
                        var _valid6 = _errs147 === errors;
                        if (_valid6 && valid50) {
                          valid50 = false;
                          passing6 = [passing6, 1];
                        } else {
                          if (_valid6) {
                            valid50 = true;
                            passing6 = 1;
                            if (props6 !== true) {
                              props6 = props6 || {};
                              props6.state = true;
                              props6.reason = true;
                            }
                          }
                          const _errs150 = errors;
                          if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                            if (data78.state !== void 0) {
                              const _errs151 = errors;
                              if ("not_applicable" !== data78.state) {
                                const err70 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err70];
                                } else {
                                  vErrors.push(err70);
                                }
                                errors++;
                              }
                              var valid53 = _errs151 === errors;
                            } else {
                              var valid53 = true;
                            }
                            if (valid53) {
                              if (data78.reason !== void 0) {
                                let data84 = data78.reason;
                                const _errs152 = errors;
                                if (!(data84 === "span_kind_not_model_backed" || data84 === "span_kind_has_no_latency" || data84 === "span_kind_has_no_token_usage" || data84 === "claude_private_lookup_not_supported" || data84 === "cursor_private_lookup_not_supported" || data84 === "codex_span_not_notify_derived" || data84 === "agent_private_lookup_not_supported")) {
                                  const err71 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err71];
                                  } else {
                                    vErrors.push(err71);
                                  }
                                  errors++;
                                }
                                var valid53 = _errs152 === errors;
                              } else {
                                var valid53 = true;
                              }
                            }
                          }
                          var _valid6 = _errs150 === errors;
                          if (_valid6 && valid50) {
                            valid50 = false;
                            passing6 = [passing6, 2];
                          } else {
                            if (_valid6) {
                              valid50 = true;
                              passing6 = 2;
                              if (props6 !== true) {
                                props6 = props6 || {};
                                props6.state = true;
                                props6.reason = true;
                              }
                            }
                            const _errs153 = errors;
                            if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                              if (data78.state !== void 0) {
                                const _errs154 = errors;
                                if ("private_lookup" !== data78.state) {
                                  const err72 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err72];
                                  } else {
                                    vErrors.push(err72);
                                  }
                                  errors++;
                                }
                                var valid54 = _errs154 === errors;
                              } else {
                                var valid54 = true;
                              }
                              if (valid54) {
                                if (data78.reason !== void 0) {
                                  const _errs155 = errors;
                                  if ("local_opt_in_lookup_required" !== data78.reason) {
                                    const err73 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err73];
                                    } else {
                                      vErrors.push(err73);
                                    }
                                    errors++;
                                  }
                                  var valid54 = _errs155 === errors;
                                } else {
                                  var valid54 = true;
                                }
                              }
                            }
                            var _valid6 = _errs153 === errors;
                            if (_valid6 && valid50) {
                              valid50 = false;
                              passing6 = [passing6, 3];
                            } else {
                              if (_valid6) {
                                valid50 = true;
                                passing6 = 3;
                                if (props6 !== true) {
                                  props6 = props6 || {};
                                  props6.state = true;
                                  props6.reason = true;
                                }
                              }
                              const _errs156 = errors;
                              if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                                if (data78.state !== void 0) {
                                  const _errs157 = errors;
                                  if ("withheld" !== data78.state) {
                                    const err74 = { instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err74];
                                    } else {
                                      vErrors.push(err74);
                                    }
                                    errors++;
                                  }
                                  var valid55 = _errs157 === errors;
                                } else {
                                  var valid55 = true;
                                }
                                if (valid55) {
                                  if (data78.reason !== void 0) {
                                    const _errs158 = errors;
                                    if ("withheld_by_privacy_policy" !== data78.reason) {
                                      const err75 = { instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err75];
                                      } else {
                                        vErrors.push(err75);
                                      }
                                      errors++;
                                    }
                                    var valid55 = _errs158 === errors;
                                  } else {
                                    var valid55 = true;
                                  }
                                }
                              }
                              var _valid6 = _errs156 === errors;
                              if (_valid6 && valid50) {
                                valid50 = false;
                                passing6 = [passing6, 4];
                              } else {
                                if (_valid6) {
                                  valid50 = true;
                                  passing6 = 4;
                                  if (props6 !== true) {
                                    props6 = props6 || {};
                                    props6.state = true;
                                    props6.reason = true;
                                  }
                                }
                              }
                            }
                          }
                        }
                        if (!valid50) {
                          const err76 = { instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing6 }, message: "must match exactly one schema in oneOf" };
                          if (vErrors === null) {
                            vErrors = [err76];
                          } else {
                            vErrors.push(err76);
                          }
                          errors++;
                          validate30.errors = vErrors;
                          return false;
                        } else {
                          errors = _errs143;
                          if (vErrors !== null) {
                            if (_errs143) {
                              vErrors.length = _errs143;
                            } else {
                              vErrors = null;
                            }
                          }
                        }
                        if (errors === _errs141) {
                          if (data78 && typeof data78 == "object" && !Array.isArray(data78)) {
                            let missing7;
                            if (data78.state === void 0 && (missing7 = "state") || data78.reason === void 0 && (missing7 = "reason")) {
                              validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing7 }, message: "must have required property '" + missing7 + "'" }];
                              return false;
                            } else {
                              const _errs159 = errors;
                              for (const key7 in data78) {
                                if (!(key7 === "state" || key7 === "reason")) {
                                  validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key7 }, message: "must NOT have additional properties" }];
                                  return false;
                                  break;
                                }
                              }
                              if (_errs159 === errors) {
                                if (data78.state !== void 0) {
                                  let data89 = data78.state;
                                  const _errs160 = errors;
                                  if (!(data89 === "available" || data89 === "source_unavailable" || data89 === "withheld" || data89 === "not_applicable" || data89 === "private_lookup")) {
                                    validate30.errors = [{ instancePath: instancePath + "/requestContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                    return false;
                                  }
                                  var valid56 = _errs160 === errors;
                                } else {
                                  var valid56 = true;
                                }
                                if (valid56) {
                                  if (data78.reason !== void 0) {
                                    let data90 = data78.reason;
                                    const _errs161 = errors;
                                    if (typeof data90 !== "string") {
                                      validate30.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    if (!(data90 === "reported_by_adapter" || data90 === "derived_from_trace_context" || data90 === "legacy_v1_report" || data90 === "source_not_provided" || data90 === "partial_token_metrics" || data90 === "historical_codex_source_not_lookup_eligible" || data90 === "codex_notify_turn_correlation_unavailable" || data90 === "ambiguous_trace_repository" || data90 === "span_kind_not_model_backed" || data90 === "span_kind_has_no_latency" || data90 === "span_kind_has_no_token_usage" || data90 === "claude_private_lookup_not_supported" || data90 === "cursor_private_lookup_not_supported" || data90 === "codex_span_not_notify_derived" || data90 === "agent_private_lookup_not_supported" || data90 === "local_opt_in_lookup_required" || data90 === "withheld_by_privacy_policy")) {
                                      validate30.errors = [{ instancePath: instancePath + "/requestContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid56 = _errs161 === errors;
                                  } else {
                                    var valid56 = true;
                                  }
                                }
                              }
                            }
                          } else {
                            validate30.errors = [{ instancePath: instancePath + "/requestContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                            return false;
                          }
                        }
                        var valid0 = _errs140 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.responseContent !== void 0) {
                          let data91 = data.responseContent;
                          const _errs163 = errors;
                          const _errs164 = errors;
                          const _errs166 = errors;
                          let valid58 = false;
                          let passing7 = null;
                          const _errs167 = errors;
                          if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                            if (data91.state !== void 0) {
                              const _errs168 = errors;
                              if ("available" !== data91.state) {
                                const err77 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/0/properties/state/const", keyword: "const", params: { allowedValue: "available" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err77];
                                } else {
                                  vErrors.push(err77);
                                }
                                errors++;
                              }
                              var valid59 = _errs168 === errors;
                            } else {
                              var valid59 = true;
                            }
                            if (valid59) {
                              if (data91.reason !== void 0) {
                                let data93 = data91.reason;
                                const _errs169 = errors;
                                if (!(data93 === "reported_by_adapter" || data93 === "derived_from_trace_context" || data93 === "legacy_v1_report")) {
                                  const err78 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/0/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[0].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err78];
                                  } else {
                                    vErrors.push(err78);
                                  }
                                  errors++;
                                }
                                var valid59 = _errs169 === errors;
                              } else {
                                var valid59 = true;
                              }
                            }
                          }
                          var _valid7 = _errs167 === errors;
                          if (_valid7) {
                            valid58 = true;
                            passing7 = 0;
                            var props7 = {};
                            props7.state = true;
                            props7.reason = true;
                          }
                          const _errs170 = errors;
                          if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                            if (data91.state !== void 0) {
                              const _errs171 = errors;
                              if ("source_unavailable" !== data91.state) {
                                const err79 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/1/properties/state/const", keyword: "const", params: { allowedValue: "source_unavailable" }, message: "must be equal to constant" };
                                if (vErrors === null) {
                                  vErrors = [err79];
                                } else {
                                  vErrors.push(err79);
                                }
                                errors++;
                              }
                              var valid60 = _errs171 === errors;
                            } else {
                              var valid60 = true;
                            }
                            if (valid60) {
                              if (data91.reason !== void 0) {
                                let data95 = data91.reason;
                                const _errs172 = errors;
                                if (!(data95 === "source_not_provided" || data95 === "partial_token_metrics" || data95 === "historical_codex_source_not_lookup_eligible" || data95 === "codex_notify_turn_correlation_unavailable" || data95 === "ambiguous_trace_repository" || data95 === "legacy_v1_report")) {
                                  const err80 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/1/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[1].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                  if (vErrors === null) {
                                    vErrors = [err80];
                                  } else {
                                    vErrors.push(err80);
                                  }
                                  errors++;
                                }
                                var valid60 = _errs172 === errors;
                              } else {
                                var valid60 = true;
                              }
                            }
                          }
                          var _valid7 = _errs170 === errors;
                          if (_valid7 && valid58) {
                            valid58 = false;
                            passing7 = [passing7, 1];
                          } else {
                            if (_valid7) {
                              valid58 = true;
                              passing7 = 1;
                              if (props7 !== true) {
                                props7 = props7 || {};
                                props7.state = true;
                                props7.reason = true;
                              }
                            }
                            const _errs173 = errors;
                            if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                              if (data91.state !== void 0) {
                                const _errs174 = errors;
                                if ("not_applicable" !== data91.state) {
                                  const err81 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/2/properties/state/const", keyword: "const", params: { allowedValue: "not_applicable" }, message: "must be equal to constant" };
                                  if (vErrors === null) {
                                    vErrors = [err81];
                                  } else {
                                    vErrors.push(err81);
                                  }
                                  errors++;
                                }
                                var valid61 = _errs174 === errors;
                              } else {
                                var valid61 = true;
                              }
                              if (valid61) {
                                if (data91.reason !== void 0) {
                                  let data97 = data91.reason;
                                  const _errs175 = errors;
                                  if (!(data97 === "span_kind_not_model_backed" || data97 === "span_kind_has_no_latency" || data97 === "span_kind_has_no_token_usage" || data97 === "claude_private_lookup_not_supported" || data97 === "cursor_private_lookup_not_supported" || data97 === "codex_span_not_notify_derived" || data97 === "agent_private_lookup_not_supported")) {
                                    const err82 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/2/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.oneOf[2].properties.reason.enum }, message: "must be equal to one of the allowed values" };
                                    if (vErrors === null) {
                                      vErrors = [err82];
                                    } else {
                                      vErrors.push(err82);
                                    }
                                    errors++;
                                  }
                                  var valid61 = _errs175 === errors;
                                } else {
                                  var valid61 = true;
                                }
                              }
                            }
                            var _valid7 = _errs173 === errors;
                            if (_valid7 && valid58) {
                              valid58 = false;
                              passing7 = [passing7, 2];
                            } else {
                              if (_valid7) {
                                valid58 = true;
                                passing7 = 2;
                                if (props7 !== true) {
                                  props7 = props7 || {};
                                  props7.state = true;
                                  props7.reason = true;
                                }
                              }
                              const _errs176 = errors;
                              if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                                if (data91.state !== void 0) {
                                  const _errs177 = errors;
                                  if ("private_lookup" !== data91.state) {
                                    const err83 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/3/properties/state/const", keyword: "const", params: { allowedValue: "private_lookup" }, message: "must be equal to constant" };
                                    if (vErrors === null) {
                                      vErrors = [err83];
                                    } else {
                                      vErrors.push(err83);
                                    }
                                    errors++;
                                  }
                                  var valid62 = _errs177 === errors;
                                } else {
                                  var valid62 = true;
                                }
                                if (valid62) {
                                  if (data91.reason !== void 0) {
                                    const _errs178 = errors;
                                    if ("local_opt_in_lookup_required" !== data91.reason) {
                                      const err84 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/3/properties/reason/const", keyword: "const", params: { allowedValue: "local_opt_in_lookup_required" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err84];
                                      } else {
                                        vErrors.push(err84);
                                      }
                                      errors++;
                                    }
                                    var valid62 = _errs178 === errors;
                                  } else {
                                    var valid62 = true;
                                  }
                                }
                              }
                              var _valid7 = _errs176 === errors;
                              if (_valid7 && valid58) {
                                valid58 = false;
                                passing7 = [passing7, 3];
                              } else {
                                if (_valid7) {
                                  valid58 = true;
                                  passing7 = 3;
                                  if (props7 !== true) {
                                    props7 = props7 || {};
                                    props7.state = true;
                                    props7.reason = true;
                                  }
                                }
                                const _errs179 = errors;
                                if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                                  if (data91.state !== void 0) {
                                    const _errs180 = errors;
                                    if ("withheld" !== data91.state) {
                                      const err85 = { instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/oneOf/4/properties/state/const", keyword: "const", params: { allowedValue: "withheld" }, message: "must be equal to constant" };
                                      if (vErrors === null) {
                                        vErrors = [err85];
                                      } else {
                                        vErrors.push(err85);
                                      }
                                      errors++;
                                    }
                                    var valid63 = _errs180 === errors;
                                  } else {
                                    var valid63 = true;
                                  }
                                  if (valid63) {
                                    if (data91.reason !== void 0) {
                                      const _errs181 = errors;
                                      if ("withheld_by_privacy_policy" !== data91.reason) {
                                        const err86 = { instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/oneOf/4/properties/reason/const", keyword: "const", params: { allowedValue: "withheld_by_privacy_policy" }, message: "must be equal to constant" };
                                        if (vErrors === null) {
                                          vErrors = [err86];
                                        } else {
                                          vErrors.push(err86);
                                        }
                                        errors++;
                                      }
                                      var valid63 = _errs181 === errors;
                                    } else {
                                      var valid63 = true;
                                    }
                                  }
                                }
                                var _valid7 = _errs179 === errors;
                                if (_valid7 && valid58) {
                                  valid58 = false;
                                  passing7 = [passing7, 4];
                                } else {
                                  if (_valid7) {
                                    valid58 = true;
                                    passing7 = 4;
                                    if (props7 !== true) {
                                      props7 = props7 || {};
                                      props7.state = true;
                                      props7.reason = true;
                                    }
                                  }
                                }
                              }
                            }
                          }
                          if (!valid58) {
                            const err87 = { instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/oneOf", keyword: "oneOf", params: { passingSchemas: passing7 }, message: "must match exactly one schema in oneOf" };
                            if (vErrors === null) {
                              vErrors = [err87];
                            } else {
                              vErrors.push(err87);
                            }
                            errors++;
                            validate30.errors = vErrors;
                            return false;
                          } else {
                            errors = _errs166;
                            if (vErrors !== null) {
                              if (_errs166) {
                                vErrors.length = _errs166;
                              } else {
                                vErrors = null;
                              }
                            }
                          }
                          if (errors === _errs164) {
                            if (data91 && typeof data91 == "object" && !Array.isArray(data91)) {
                              let missing8;
                              if (data91.state === void 0 && (missing8 = "state") || data91.reason === void 0 && (missing8 = "reason")) {
                                validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/required", keyword: "required", params: { missingProperty: missing8 }, message: "must have required property '" + missing8 + "'" }];
                                return false;
                              } else {
                                const _errs182 = errors;
                                for (const key8 in data91) {
                                  if (!(key8 === "state" || key8 === "reason")) {
                                    validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key8 }, message: "must NOT have additional properties" }];
                                    return false;
                                    break;
                                  }
                                }
                                if (_errs182 === errors) {
                                  if (data91.state !== void 0) {
                                    let data102 = data91.state;
                                    const _errs183 = errors;
                                    if (!(data102 === "available" || data102 === "source_unavailable" || data102 === "withheld" || data102 === "not_applicable" || data102 === "private_lookup")) {
                                      validate30.errors = [{ instancePath: instancePath + "/responseContent/state", schemaPath: "#/$defs/field_availability/properties/state/enum", keyword: "enum", params: { allowedValues: schema50.properties.state.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid64 = _errs183 === errors;
                                  } else {
                                    var valid64 = true;
                                  }
                                  if (valid64) {
                                    if (data91.reason !== void 0) {
                                      let data103 = data91.reason;
                                      const _errs184 = errors;
                                      if (typeof data103 !== "string") {
                                        validate30.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      if (!(data103 === "reported_by_adapter" || data103 === "derived_from_trace_context" || data103 === "legacy_v1_report" || data103 === "source_not_provided" || data103 === "partial_token_metrics" || data103 === "historical_codex_source_not_lookup_eligible" || data103 === "codex_notify_turn_correlation_unavailable" || data103 === "ambiguous_trace_repository" || data103 === "span_kind_not_model_backed" || data103 === "span_kind_has_no_latency" || data103 === "span_kind_has_no_token_usage" || data103 === "claude_private_lookup_not_supported" || data103 === "cursor_private_lookup_not_supported" || data103 === "codex_span_not_notify_derived" || data103 === "agent_private_lookup_not_supported" || data103 === "local_opt_in_lookup_required" || data103 === "withheld_by_privacy_policy")) {
                                        validate30.errors = [{ instancePath: instancePath + "/responseContent/reason", schemaPath: "#/$defs/field_availability/properties/reason/enum", keyword: "enum", params: { allowedValues: schema50.properties.reason.enum }, message: "must be equal to one of the allowed values" }];
                                        return false;
                                      }
                                      var valid64 = _errs184 === errors;
                                    } else {
                                      var valid64 = true;
                                    }
                                  }
                                }
                              }
                            } else {
                              validate30.errors = [{ instancePath: instancePath + "/responseContent", schemaPath: "#/$defs/field_availability/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                              return false;
                            }
                          }
                          var valid0 = _errs163 === errors;
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
