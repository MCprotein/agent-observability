/* Generated from contracts/report-dto-v1.schema.json. Do not edit. */
(() => {
  // ui/report/generated/validate-report-dto-v1.js
  var validate_report_dto_v1_default = validate20;
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
  var schema47 = { "type": "object", "additionalProperties": false, "required": ["schemaVersion", "traceId", "spanId", "parentSpanId", "kind", "name", "status", "startTimeUnixMs", "endTimeUnixMs", "repo", "agent", "attributes", "metrics", "cost"], "properties": { "schemaVersion": { "type": "string" }, "traceId": { "type": "string" }, "spanId": { "type": "string" }, "parentSpanId": { "type": ["string", "null"] }, "kind": { "type": "string" }, "name": { "type": "string" }, "status": { "type": "string" }, "startTimeUnixMs": { "type": "number" }, "endTimeUnixMs": { "type": ["number", "null"] }, "repo": { "type": "string" }, "agent": { "$ref": "#/$defs/agent" }, "sessionId": { "type": "string" }, "turnId": { "type": "string" }, "toolName": { "type": "string" }, "attributes": { "$ref": "#/$defs/attributes" }, "metrics": { "$ref": "#/$defs/metrics" }, "estimatedCost": { "type": "number" }, "cost": { "$ref": "#/$defs/cost" } } };
  var schema62 = { "type": "object", "additionalProperties": false, "properties": { "inputTokens": { "type": "number" }, "outputTokens": { "type": "number" }, "cachedInputTokens": { "type": "number" }, "cacheCreationInputTokens": { "type": "number" }, "reasoningOutputTokens": { "type": "number" }, "totalTokens": { "type": "number" }, "latencyMs": { "type": "number" }, "durationMs": { "type": "number" }, "totalInputTokens": { "type": "number" }, "totalOutputTokens": { "type": "number" }, "totalCachedInputTokens": { "type": "number" }, "totalReasoningOutputTokens": { "type": "number" }, "totalAccumulatedTokens": { "type": "number" }, "contextWindowTokens": { "type": "number" } } };
  var schema49 = { "type": "object", "additionalProperties": false, "properties": { "source": { "$ref": "#/$defs/scalar" }, "event_type": { "$ref": "#/$defs/scalar" }, "envelope_type": { "$ref": "#/$defs/scalar" }, "session_id": { "$ref": "#/$defs/scalar" }, "turn_id": { "$ref": "#/$defs/scalar" }, "request_id": { "$ref": "#/$defs/scalar" }, "call_id": { "$ref": "#/$defs/scalar" }, "tool_name": { "$ref": "#/$defs/scalar" }, "phase": { "$ref": "#/$defs/scalar" }, "exit_code": { "$ref": "#/$defs/scalar" }, "sandbox": { "$ref": "#/$defs/scalar" }, "approval": { "$ref": "#/$defs/scalar" } } };
  var schema50 = { "type": ["string", "number", "boolean"] };
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
        const _errs1 = errors;
        for (const key0 in data) {
          if (!func1.call(schema49.properties, key0)) {
            validate30.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
            return false;
            break;
          }
        }
        if (_errs1 === errors) {
          if (data.source !== void 0) {
            let data0 = data.source;
            const _errs2 = errors;
            if (typeof data0 !== "string" && !(typeof data0 == "number" && isFinite(data0)) && typeof data0 !== "boolean") {
              validate30.errors = [{ instancePath: instancePath + "/source", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                validate30.errors = [{ instancePath: instancePath + "/event_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                  validate30.errors = [{ instancePath: instancePath + "/envelope_type", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                    validate30.errors = [{ instancePath: instancePath + "/session_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                      validate30.errors = [{ instancePath: instancePath + "/turn_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                        validate30.errors = [{ instancePath: instancePath + "/request_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                          validate30.errors = [{ instancePath: instancePath + "/call_id", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                            validate30.errors = [{ instancePath: instancePath + "/tool_name", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                              validate30.errors = [{ instancePath: instancePath + "/phase", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                                validate30.errors = [{ instancePath: instancePath + "/exit_code", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                                  validate30.errors = [{ instancePath: instancePath + "/sandbox", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
                                    validate30.errors = [{ instancePath: instancePath + "/approval", schemaPath: "#/$defs/scalar/type", keyword: "type", params: { type: schema50.type }, message: "must be string,number,boolean" }];
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
        validate30.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    validate30.errors = vErrors;
    return errors === 0;
  }
  validate30.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };
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
    if (errors === 0) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing0;
        if (data.schemaVersion === void 0 && (missing0 = "schemaVersion") || data.traceId === void 0 && (missing0 = "traceId") || data.spanId === void 0 && (missing0 = "spanId") || data.parentSpanId === void 0 && (missing0 = "parentSpanId") || data.kind === void 0 && (missing0 = "kind") || data.name === void 0 && (missing0 = "name") || data.status === void 0 && (missing0 = "status") || data.startTimeUnixMs === void 0 && (missing0 = "startTimeUnixMs") || data.endTimeUnixMs === void 0 && (missing0 = "endTimeUnixMs") || data.repo === void 0 && (missing0 = "repo") || data.agent === void 0 && (missing0 = "agent") || data.attributes === void 0 && (missing0 = "attributes") || data.metrics === void 0 && (missing0 = "metrics") || data.cost === void 0 && (missing0 = "cost")) {
          validate29.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          const _errs1 = errors;
          for (const key0 in data) {
            if (!func1.call(schema47.properties, key0)) {
              validate29.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
              return false;
              break;
            }
          }
          if (_errs1 === errors) {
            if (data.schemaVersion !== void 0) {
              const _errs2 = errors;
              if (typeof data.schemaVersion !== "string") {
                validate29.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                return false;
              }
              var valid0 = _errs2 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.traceId !== void 0) {
                const _errs4 = errors;
                if (typeof data.traceId !== "string") {
                  validate29.errors = [{ instancePath: instancePath + "/traceId", schemaPath: "#/properties/traceId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                var valid0 = _errs4 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.spanId !== void 0) {
                  const _errs6 = errors;
                  if (typeof data.spanId !== "string") {
                    validate29.errors = [{ instancePath: instancePath + "/spanId", schemaPath: "#/properties/spanId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                  var valid0 = _errs6 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.parentSpanId !== void 0) {
                    let data3 = data.parentSpanId;
                    const _errs8 = errors;
                    if (typeof data3 !== "string" && data3 !== null) {
                      validate29.errors = [{ instancePath: instancePath + "/parentSpanId", schemaPath: "#/properties/parentSpanId/type", keyword: "type", params: { type: schema47.properties.parentSpanId.type }, message: "must be string,null" }];
                      return false;
                    }
                    var valid0 = _errs8 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.kind !== void 0) {
                      const _errs10 = errors;
                      if (typeof data.kind !== "string") {
                        validate29.errors = [{ instancePath: instancePath + "/kind", schemaPath: "#/properties/kind/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                        return false;
                      }
                      var valid0 = _errs10 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.name !== void 0) {
                        const _errs12 = errors;
                        if (typeof data.name !== "string") {
                          validate29.errors = [{ instancePath: instancePath + "/name", schemaPath: "#/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                          return false;
                        }
                        var valid0 = _errs12 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.status !== void 0) {
                          const _errs14 = errors;
                          if (typeof data.status !== "string") {
                            validate29.errors = [{ instancePath: instancePath + "/status", schemaPath: "#/properties/status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
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
                              validate29.errors = [{ instancePath: instancePath + "/startTimeUnixMs", schemaPath: "#/properties/startTimeUnixMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
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
                                validate29.errors = [{ instancePath: instancePath + "/endTimeUnixMs", schemaPath: "#/properties/endTimeUnixMs/type", keyword: "type", params: { type: schema47.properties.endTimeUnixMs.type }, message: "must be number,null" }];
                                return false;
                              }
                              var valid0 = _errs18 === errors;
                            } else {
                              var valid0 = true;
                            }
                            if (valid0) {
                              if (data.repo !== void 0) {
                                const _errs20 = errors;
                                if (typeof data.repo !== "string") {
                                  validate29.errors = [{ instancePath: instancePath + "/repo", schemaPath: "#/properties/repo/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                var valid0 = _errs20 === errors;
                              } else {
                                var valid0 = true;
                              }
                              if (valid0) {
                                if (data.agent !== void 0) {
                                  let data10 = data.agent;
                                  const _errs22 = errors;
                                  const _errs23 = errors;
                                  if (errors === _errs23) {
                                    if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                                      const _errs25 = errors;
                                      for (const key1 in data10) {
                                        if (!(key1 === "name" || key1 === "model" || key1 === "version")) {
                                          validate29.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                                          return false;
                                          break;
                                        }
                                      }
                                      if (_errs25 === errors) {
                                        if (data10.name !== void 0) {
                                          const _errs26 = errors;
                                          if (typeof data10.name !== "string") {
                                            validate29.errors = [{ instancePath: instancePath + "/agent/name", schemaPath: "#/$defs/agent/properties/name/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                            return false;
                                          }
                                          var valid2 = _errs26 === errors;
                                        } else {
                                          var valid2 = true;
                                        }
                                        if (valid2) {
                                          if (data10.model !== void 0) {
                                            const _errs28 = errors;
                                            if (typeof data10.model !== "string") {
                                              validate29.errors = [{ instancePath: instancePath + "/agent/model", schemaPath: "#/$defs/agent/properties/model/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                              return false;
                                            }
                                            var valid2 = _errs28 === errors;
                                          } else {
                                            var valid2 = true;
                                          }
                                          if (valid2) {
                                            if (data10.version !== void 0) {
                                              const _errs30 = errors;
                                              if (typeof data10.version !== "string") {
                                                validate29.errors = [{ instancePath: instancePath + "/agent/version", schemaPath: "#/$defs/agent/properties/version/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                                return false;
                                              }
                                              var valid2 = _errs30 === errors;
                                            } else {
                                              var valid2 = true;
                                            }
                                          }
                                        }
                                      }
                                    } else {
                                      validate29.errors = [{ instancePath: instancePath + "/agent", schemaPath: "#/$defs/agent/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                                      return false;
                                    }
                                  }
                                  var valid0 = _errs22 === errors;
                                } else {
                                  var valid0 = true;
                                }
                                if (valid0) {
                                  if (data.sessionId !== void 0) {
                                    const _errs32 = errors;
                                    if (typeof data.sessionId !== "string") {
                                      validate29.errors = [{ instancePath: instancePath + "/sessionId", schemaPath: "#/properties/sessionId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    var valid0 = _errs32 === errors;
                                  } else {
                                    var valid0 = true;
                                  }
                                  if (valid0) {
                                    if (data.turnId !== void 0) {
                                      const _errs34 = errors;
                                      if (typeof data.turnId !== "string") {
                                        validate29.errors = [{ instancePath: instancePath + "/turnId", schemaPath: "#/properties/turnId/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                        return false;
                                      }
                                      var valid0 = _errs34 === errors;
                                    } else {
                                      var valid0 = true;
                                    }
                                    if (valid0) {
                                      if (data.toolName !== void 0) {
                                        const _errs36 = errors;
                                        if (typeof data.toolName !== "string") {
                                          validate29.errors = [{ instancePath: instancePath + "/toolName", schemaPath: "#/properties/toolName/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                          return false;
                                        }
                                        var valid0 = _errs36 === errors;
                                      } else {
                                        var valid0 = true;
                                      }
                                      if (valid0) {
                                        if (data.attributes !== void 0) {
                                          const _errs38 = errors;
                                          if (!validate30(data.attributes, { instancePath: instancePath + "/attributes", parentData: data, parentDataProperty: "attributes", rootData, dynamicAnchors })) {
                                            vErrors = vErrors === null ? validate30.errors : vErrors.concat(validate30.errors);
                                            errors = vErrors.length;
                                          }
                                          var valid0 = _errs38 === errors;
                                        } else {
                                          var valid0 = true;
                                        }
                                        if (valid0) {
                                          if (data.metrics !== void 0) {
                                            let data18 = data.metrics;
                                            const _errs39 = errors;
                                            const _errs40 = errors;
                                            if (errors === _errs40) {
                                              if (data18 && typeof data18 == "object" && !Array.isArray(data18)) {
                                                const _errs42 = errors;
                                                for (const key2 in data18) {
                                                  if (!func1.call(schema62.properties, key2)) {
                                                    validate29.errors = [{ instancePath: instancePath + "/metrics", schemaPath: "#/$defs/metrics/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                                    return false;
                                                    break;
                                                  }
                                                }
                                                if (_errs42 === errors) {
                                                  if (data18.inputTokens !== void 0) {
                                                    let data19 = data18.inputTokens;
                                                    const _errs43 = errors;
                                                    if (!(typeof data19 == "number" && isFinite(data19))) {
                                                      validate29.errors = [{ instancePath: instancePath + "/metrics/inputTokens", schemaPath: "#/$defs/metrics/properties/inputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                      return false;
                                                    }
                                                    var valid4 = _errs43 === errors;
                                                  } else {
                                                    var valid4 = true;
                                                  }
                                                  if (valid4) {
                                                    if (data18.outputTokens !== void 0) {
                                                      let data20 = data18.outputTokens;
                                                      const _errs45 = errors;
                                                      if (!(typeof data20 == "number" && isFinite(data20))) {
                                                        validate29.errors = [{ instancePath: instancePath + "/metrics/outputTokens", schemaPath: "#/$defs/metrics/properties/outputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                        return false;
                                                      }
                                                      var valid4 = _errs45 === errors;
                                                    } else {
                                                      var valid4 = true;
                                                    }
                                                    if (valid4) {
                                                      if (data18.cachedInputTokens !== void 0) {
                                                        let data21 = data18.cachedInputTokens;
                                                        const _errs47 = errors;
                                                        if (!(typeof data21 == "number" && isFinite(data21))) {
                                                          validate29.errors = [{ instancePath: instancePath + "/metrics/cachedInputTokens", schemaPath: "#/$defs/metrics/properties/cachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                          return false;
                                                        }
                                                        var valid4 = _errs47 === errors;
                                                      } else {
                                                        var valid4 = true;
                                                      }
                                                      if (valid4) {
                                                        if (data18.cacheCreationInputTokens !== void 0) {
                                                          let data22 = data18.cacheCreationInputTokens;
                                                          const _errs49 = errors;
                                                          if (!(typeof data22 == "number" && isFinite(data22))) {
                                                            validate29.errors = [{ instancePath: instancePath + "/metrics/cacheCreationInputTokens", schemaPath: "#/$defs/metrics/properties/cacheCreationInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                            return false;
                                                          }
                                                          var valid4 = _errs49 === errors;
                                                        } else {
                                                          var valid4 = true;
                                                        }
                                                        if (valid4) {
                                                          if (data18.reasoningOutputTokens !== void 0) {
                                                            let data23 = data18.reasoningOutputTokens;
                                                            const _errs51 = errors;
                                                            if (!(typeof data23 == "number" && isFinite(data23))) {
                                                              validate29.errors = [{ instancePath: instancePath + "/metrics/reasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/reasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                              return false;
                                                            }
                                                            var valid4 = _errs51 === errors;
                                                          } else {
                                                            var valid4 = true;
                                                          }
                                                          if (valid4) {
                                                            if (data18.totalTokens !== void 0) {
                                                              let data24 = data18.totalTokens;
                                                              const _errs53 = errors;
                                                              if (!(typeof data24 == "number" && isFinite(data24))) {
                                                                validate29.errors = [{ instancePath: instancePath + "/metrics/totalTokens", schemaPath: "#/$defs/metrics/properties/totalTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                return false;
                                                              }
                                                              var valid4 = _errs53 === errors;
                                                            } else {
                                                              var valid4 = true;
                                                            }
                                                            if (valid4) {
                                                              if (data18.latencyMs !== void 0) {
                                                                let data25 = data18.latencyMs;
                                                                const _errs55 = errors;
                                                                if (!(typeof data25 == "number" && isFinite(data25))) {
                                                                  validate29.errors = [{ instancePath: instancePath + "/metrics/latencyMs", schemaPath: "#/$defs/metrics/properties/latencyMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                  return false;
                                                                }
                                                                var valid4 = _errs55 === errors;
                                                              } else {
                                                                var valid4 = true;
                                                              }
                                                              if (valid4) {
                                                                if (data18.durationMs !== void 0) {
                                                                  let data26 = data18.durationMs;
                                                                  const _errs57 = errors;
                                                                  if (!(typeof data26 == "number" && isFinite(data26))) {
                                                                    validate29.errors = [{ instancePath: instancePath + "/metrics/durationMs", schemaPath: "#/$defs/metrics/properties/durationMs/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                    return false;
                                                                  }
                                                                  var valid4 = _errs57 === errors;
                                                                } else {
                                                                  var valid4 = true;
                                                                }
                                                                if (valid4) {
                                                                  if (data18.totalInputTokens !== void 0) {
                                                                    let data27 = data18.totalInputTokens;
                                                                    const _errs59 = errors;
                                                                    if (!(typeof data27 == "number" && isFinite(data27))) {
                                                                      validate29.errors = [{ instancePath: instancePath + "/metrics/totalInputTokens", schemaPath: "#/$defs/metrics/properties/totalInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                      return false;
                                                                    }
                                                                    var valid4 = _errs59 === errors;
                                                                  } else {
                                                                    var valid4 = true;
                                                                  }
                                                                  if (valid4) {
                                                                    if (data18.totalOutputTokens !== void 0) {
                                                                      let data28 = data18.totalOutputTokens;
                                                                      const _errs61 = errors;
                                                                      if (!(typeof data28 == "number" && isFinite(data28))) {
                                                                        validate29.errors = [{ instancePath: instancePath + "/metrics/totalOutputTokens", schemaPath: "#/$defs/metrics/properties/totalOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                        return false;
                                                                      }
                                                                      var valid4 = _errs61 === errors;
                                                                    } else {
                                                                      var valid4 = true;
                                                                    }
                                                                    if (valid4) {
                                                                      if (data18.totalCachedInputTokens !== void 0) {
                                                                        let data29 = data18.totalCachedInputTokens;
                                                                        const _errs63 = errors;
                                                                        if (!(typeof data29 == "number" && isFinite(data29))) {
                                                                          validate29.errors = [{ instancePath: instancePath + "/metrics/totalCachedInputTokens", schemaPath: "#/$defs/metrics/properties/totalCachedInputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                          return false;
                                                                        }
                                                                        var valid4 = _errs63 === errors;
                                                                      } else {
                                                                        var valid4 = true;
                                                                      }
                                                                      if (valid4) {
                                                                        if (data18.totalReasoningOutputTokens !== void 0) {
                                                                          let data30 = data18.totalReasoningOutputTokens;
                                                                          const _errs65 = errors;
                                                                          if (!(typeof data30 == "number" && isFinite(data30))) {
                                                                            validate29.errors = [{ instancePath: instancePath + "/metrics/totalReasoningOutputTokens", schemaPath: "#/$defs/metrics/properties/totalReasoningOutputTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                            return false;
                                                                          }
                                                                          var valid4 = _errs65 === errors;
                                                                        } else {
                                                                          var valid4 = true;
                                                                        }
                                                                        if (valid4) {
                                                                          if (data18.totalAccumulatedTokens !== void 0) {
                                                                            let data31 = data18.totalAccumulatedTokens;
                                                                            const _errs67 = errors;
                                                                            if (!(typeof data31 == "number" && isFinite(data31))) {
                                                                              validate29.errors = [{ instancePath: instancePath + "/metrics/totalAccumulatedTokens", schemaPath: "#/$defs/metrics/properties/totalAccumulatedTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                              return false;
                                                                            }
                                                                            var valid4 = _errs67 === errors;
                                                                          } else {
                                                                            var valid4 = true;
                                                                          }
                                                                          if (valid4) {
                                                                            if (data18.contextWindowTokens !== void 0) {
                                                                              let data32 = data18.contextWindowTokens;
                                                                              const _errs69 = errors;
                                                                              if (!(typeof data32 == "number" && isFinite(data32))) {
                                                                                validate29.errors = [{ instancePath: instancePath + "/metrics/contextWindowTokens", schemaPath: "#/$defs/metrics/properties/contextWindowTokens/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                                                return false;
                                                                              }
                                                                              var valid4 = _errs69 === errors;
                                                                            } else {
                                                                              var valid4 = true;
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
                                            var valid0 = _errs39 === errors;
                                          } else {
                                            var valid0 = true;
                                          }
                                          if (valid0) {
                                            if (data.estimatedCost !== void 0) {
                                              let data33 = data.estimatedCost;
                                              const _errs71 = errors;
                                              if (!(typeof data33 == "number" && isFinite(data33))) {
                                                validate29.errors = [{ instancePath: instancePath + "/estimatedCost", schemaPath: "#/properties/estimatedCost/type", keyword: "type", params: { type: "number" }, message: "must be number" }];
                                                return false;
                                              }
                                              var valid0 = _errs71 === errors;
                                            } else {
                                              var valid0 = true;
                                            }
                                            if (valid0) {
                                              if (data.cost !== void 0) {
                                                const _errs73 = errors;
                                                if (!validate21(data.cost, { instancePath: instancePath + "/cost", parentData: data, parentDataProperty: "cost", rootData, dynamicAnchors })) {
                                                  vErrors = vErrors === null ? validate21.errors : vErrors.concat(validate21.errors);
                                                  errors = vErrors.length;
                                                }
                                                var valid0 = _errs73 === errors;
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
              if ("agent_observability.report.v1" !== data.schemaVersion) {
                validate20.errors = [{ instancePath: instancePath + "/schemaVersion", schemaPath: "#/properties/schemaVersion/const", keyword: "const", params: { allowedValue: "agent_observability.report.v1" }, message: "must be equal to constant" }];
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

  // ui/report/view-summary.ts
  var TOKEN_METRICS = [
    "inputTokens",
    "outputTokens",
    "cachedInputTokens",
    "cacheCreationInputTokens",
    "reasoningOutputTokens"
  ];
  function summarizeVisible(spans) {
    const sessions = /* @__PURE__ */ new Set();
    const turns = /* @__PURE__ */ new Set();
    const billable = spans.filter(hasTokenMetrics);
    const summary = {
      sessions: 0,
      turns: 0,
      llmRequests: 0,
      toolExecutions: 0,
      errors: 0,
      inputTokens: 0,
      outputTokens: 0,
      estimatedCost: 0,
      costStatus: aggregateCostStatus(billable.map((span) => span.cost.status))
    };
    for (const span of spans) {
      if (span.sessionId) sessions.add(span.sessionId);
      if (span.turnId) turns.add(span.turnId);
      if (span.kind === "llm.request") summary.llmRequests += 1;
      if (span.kind === "tool.execution") summary.toolExecutions += 1;
      if (span.status === "error") summary.errors += 1;
      summary.inputTokens += span.metrics.inputTokens ?? 0;
      summary.outputTokens += span.metrics.outputTokens ?? 0;
      summary.estimatedCost += span.estimatedCost ?? 0;
    }
    summary.sessions = sessions.size;
    summary.turns = turns.size;
    return summary;
  }
  function aggregateCostStatus(statuses) {
    const estimated = statuses.filter((status) => status === "estimated").length;
    const incomplete = statuses.filter((status) => status === "incomplete").length;
    const unknown = statuses.filter((status) => status === "unknown").length;
    if (estimated === 0 && incomplete === 0) return "unknown";
    if (incomplete > 0 || unknown > 0) return "incomplete";
    return "estimated";
  }
  function hasTokenMetrics(span) {
    return TOKEN_METRICS.some((key) => span.metrics[key] !== void 0);
  }

  // ui/report/main.ts
  var ALL_OPTION = "-1";
  var UNKNOWN = "unknown";
  var reportData = document.getElementById("report-data");
  if (!reportData) {
    throw new Error("Missing report data");
  }
  var candidate = parseReportData(reportData.textContent);
  if (candidate === void 0 || !validate_report_dto_v1_default(candidate)) {
    document.body.replaceChildren(errorState("Report data does not match agent_observability.report.v1."));
  } else {
    mount(candidate);
  }
  function parseReportData(value) {
    try {
      return JSON.parse(value ?? "null");
    } catch {
      return void 0;
    }
  }
  function mount(data) {
    const state = {
      repo: void 0,
      session: void 0,
      agent: void 0,
      model: void 0,
      text: "",
      trace: void 0
    };
    const selects = {
      repo: element("repo-filter"),
      session: element("session-filter"),
      agent: element("agent-filter"),
      model: element("model-filter")
    };
    const textFilter = element("text-filter");
    const clearFilters = element("clear-filters");
    const tracesElement = element("trace-list");
    const tableElement = element("span-table");
    const traceCount = element("trace-count");
    const spanCount = element("span-count");
    const filterStatus = element("filter-status");
    const filterValues = {
      repo: data.filters.repos,
      session: data.filters.sessions,
      agent: data.filters.agents ?? uniqueSorted(data.spans.map((span) => span.agent.name ?? UNKNOWN)),
      model: data.filters.models ?? uniqueSorted(data.spans.map((span) => span.agent.model ?? UNKNOWN))
    };
    fillSelect(selects.repo, filterValues.repo, "All repos");
    fillSelect(selects.session, filterValues.session, "All sessions");
    fillSelect(selects.agent, filterValues.agent, "All agents");
    fillSelect(selects.model, filterValues.model, "All models");
    for (const key of Object.keys(selects)) {
      selects[key].addEventListener("change", () => {
        const index = Number(selects[key].value);
        state[key] = index >= 0 ? filterValues[key][index] : void 0;
        state.trace = void 0;
        render();
      });
    }
    textFilter.addEventListener("input", () => {
      state.text = textFilter.value.trim().toLowerCase();
      state.trace = void 0;
      render();
    });
    clearFilters.addEventListener("click", () => {
      for (const key of Object.keys(selects)) {
        state[key] = void 0;
        selects[key].value = ALL_OPTION;
      }
      state.text = "";
      state.trace = void 0;
      textFilter.value = "";
      render();
      selects.repo.focus();
    });
    render();
    function render() {
      const spans = data.spans.filter(matchesFilters);
      const traces = data.traces.filter((trace) => spans.some((span) => span.traceId === trace.traceId));
      if (state.trace !== void 0 && !traces.some((trace) => trace.traceId === state.trace)) {
        state.trace = void 0;
      }
      const visibleSpans = state.trace === void 0 ? spans : spans.filter((span) => span.traceId === state.trace);
      const summary = summarizeVisible(visibleSpans);
      setText("kpi-sessions", summary.sessions);
      setText("kpi-turns", summary.turns);
      setText("kpi-llm", summary.llmRequests);
      setText("kpi-tools", summary.toolExecutions);
      setText("kpi-tokens", formatNumber(summary.inputTokens + summary.outputTokens));
      setText("kpi-cost", formatCost(summary.estimatedCost, {
        status: summary.costStatus,
        currency: data.cost.currency
      }));
      setText("kpi-errors", summary.errors);
      traceCount.textContent = String(traces.length);
      spanCount.textContent = String(visibleSpans.length);
      clearFilters.disabled = !hasActiveFilters();
      filterStatus.textContent = hasActiveFilters() ? `${visibleSpans.length} spans match the active filters.` : `${visibleSpans.length} spans in this local report.`;
      renderTraces(traces, spans);
      renderSpans(visibleSpans);
    }
    function matchesFilters(span) {
      if (state.repo !== void 0 && span.repo !== state.repo) return false;
      if (state.session !== void 0 && span.sessionId !== state.session) return false;
      if (state.agent !== void 0 && (span.agent.name ?? UNKNOWN) !== state.agent) return false;
      if (state.model !== void 0 && (span.agent.model ?? UNKNOWN) !== state.model) return false;
      if (!state.text) return true;
      return [
        span.name,
        span.kind,
        span.status,
        span.toolName,
        span.traceId,
        span.spanId,
        span.agent.name,
        span.agent.model
      ].join(" ").toLowerCase().includes(state.text);
    }
    function renderTraces(traces, filteredSpans) {
      if (traces.length === 0) {
        tracesElement.innerHTML = '<div class="empty">No traces match the active filters.</div>';
        return;
      }
      tracesElement.replaceChildren(...traces.map((trace) => {
        const traceSpans = spansForTrace(trace, filteredSpans);
        const traceSummary = summarizeVisible(traceSpans);
        const button = document.createElement("button");
        button.type = "button";
        button.className = `trace-row${state.trace === trace.traceId ? " active" : ""}`;
        button.setAttribute("aria-pressed", String(state.trace === trace.traceId));
        button.addEventListener("click", () => {
          state.trace = state.trace === trace.traceId ? void 0 : trace.traceId;
          render();
        });
        button.innerHTML = `<div class="trace-main"><span class="mono">${escapeHtml(shortId(trace.traceId))}</span><span class="badge ${traceSummary.errors ? "error" : "ok"}">${traceSummary.errors ? `${traceSummary.errors} error` : "ok"}</span></div><div class="trace-meta"><span>${escapeHtml(trace.repo)}</span><span>${traceSpans.length} spans</span><span>${formatNumber(traceSummary.inputTokens + traceSummary.outputTokens)} tokens</span></div>`;
        return button;
      }));
    }
    function renderSpans(spans) {
      if (spans.length === 0) {
        tableElement.innerHTML = '<tr><td class="empty" colspan="9">No spans match the active filters.</td></tr>';
        return;
      }
      tableElement.replaceChildren(...spans.map((span) => {
        const row = document.createElement("tr");
        row.innerHTML = `<td><span class="badge">${escapeHtml(span.kind)}</span></td><td>${escapeHtml(span.name)}${span.toolName ? `<div class="mono">${escapeHtml(span.toolName)}</div>` : ""}</td><td><span class="badge ${statusClass(span.status)}">${escapeHtml(span.status)}</span></td><td>${escapeHtml(span.repo)}</td><td class="mono">${escapeHtml(span.turnId ?? "")}</td><td>${formatNumber((span.metrics.inputTokens ?? 0) + (span.metrics.outputTokens ?? 0))}</td><td>${escapeHtml(formatCost(span.estimatedCost, span.cost))}</td><td>${formatDuration(span.metrics.latencyMs ?? span.metrics.durationMs)}</td><td class="mono">${escapeHtml(shortId(span.parentSpanId ?? ""))}</td>`;
        return row;
      }));
    }
    function hasActiveFilters() {
      return state.text.length > 0 || Object.keys(selects).some((key) => state[key] !== void 0);
    }
  }
  function spansForTrace(trace, spans) {
    return spans.filter((span) => span.traceId === trace.traceId);
  }
  function element(id) {
    const value = document.getElementById(id);
    if (!value) throw new Error(`Missing report element: ${id}`);
    return value;
  }
  function fillSelect(select, values, allLabel) {
    const options = [
      option(ALL_OPTION, allLabel),
      ...values.map((value, index) => option(String(index), value === UNKNOWN ? "Unknown" : value))
    ];
    select.replaceChildren(...options);
  }
  function option(value, label) {
    const result = document.createElement("option");
    result.value = value;
    result.textContent = label;
    return result;
  }
  function uniqueSorted(values) {
    return [...new Set(values)].sort();
  }
  function setText(id, value) {
    element(id).textContent = typeof value === "number" ? formatNumber(value) : value;
  }
  function formatNumber(value) {
    return Number(value || 0).toLocaleString();
  }
  function formatDuration(value) {
    return Number.isFinite(value) ? `${Number(value).toLocaleString()} ms` : "";
  }
  function formatCost(value, cost) {
    if (cost.status === "unknown" && (!Number.isFinite(value) || value === 0)) return "unknown";
    if (!Number.isFinite(value)) return cost.status;
    const amount = `${cost.currency ?? "USD"} ${Number(Number(value).toPrecision(12)).toString()}`;
    return cost.status === "incomplete" ? `${amount} incomplete` : amount;
  }
  function statusClass(status) {
    if (status === "error") return "error";
    if (status === "ok") return "ok";
    return "warning";
  }
  function shortId(value) {
    return value.length > 18 ? `${value.slice(0, 8)}...${value.slice(-6)}` : value;
  }
  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (char) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;"
    })[char] ?? char);
  }
  function errorState(message) {
    const element2 = document.createElement("main");
    element2.className = "report-error";
    element2.textContent = message;
    return element2;
  }
})();
