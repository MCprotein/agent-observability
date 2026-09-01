/* Generated from contracts/local-runtime-config-v2.schema.json. Do not edit. */
(() => {
  // node_modules/lucide/dist/esm/defaultAttributes.mjs
  var defaultAttributes = {
    xmlns: "http://www.w3.org/2000/svg",
    width: 24,
    height: 24,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": 2,
    "stroke-linecap": "round",
    "stroke-linejoin": "round"
  };

  // node_modules/lucide/dist/esm/createElement.mjs
  var createSVGElement = ([tag, attrs, children]) => {
    const element = document.createElementNS("http://www.w3.org/2000/svg", tag);
    Object.keys(attrs).forEach((name) => {
      element.setAttribute(name, String(attrs[name]));
    });
    if (children?.length) {
      children.forEach((child) => {
        const childElement = createSVGElement(child);
        element.appendChild(childElement);
      });
    }
    return element;
  };
  var createElement = (iconNode, customAttrs = {}) => {
    const tag = "svg";
    const attrs = {
      ...defaultAttributes,
      ...customAttrs
    };
    return createSVGElement([tag, attrs, iconNode]);
  };

  // node_modules/lucide/dist/esm/shared/src/utils/hasA11yProp.mjs
  var hasA11yProp = (props) => {
    for (const prop in props) {
      if (prop.startsWith("aria-") || prop === "role" || prop === "title") {
        return true;
      }
    }
    return false;
  };

  // node_modules/lucide/dist/esm/shared/src/utils/mergeClasses.mjs
  var mergeClasses = (...classes) => classes.filter((className, index, array) => {
    return Boolean(className) && className.trim() !== "" && array.indexOf(className) === index;
  }).join(" ").trim();

  // node_modules/lucide/dist/esm/shared/src/utils/toCamelCase.mjs
  var toCamelCase = (string) => string.replace(
    /^([A-Z])|[\s-_]+(\w)/g,
    (match, p1, p2) => p2 ? p2.toUpperCase() : p1.toLowerCase()
  );

  // node_modules/lucide/dist/esm/shared/src/utils/toPascalCase.mjs
  var toPascalCase = (string) => {
    const camelCase = toCamelCase(string);
    return camelCase.charAt(0).toUpperCase() + camelCase.slice(1);
  };

  // node_modules/lucide/dist/esm/replaceElement.mjs
  var getAttrs = (element) => Array.from(element.attributes).reduce((attrs, attr) => {
    attrs[attr.name] = attr.value;
    return attrs;
  }, {});
  var getClassNames = (attrs) => {
    if (typeof attrs === "string") return attrs;
    if (!attrs || !attrs.class) return "";
    if (attrs.class && typeof attrs.class === "string") {
      return attrs.class.split(" ");
    }
    if (attrs.class && Array.isArray(attrs.class)) {
      return attrs.class;
    }
    return "";
  };
  var replaceElement = (element, { nameAttr, icons, attrs }) => {
    const iconName = element.getAttribute(nameAttr);
    if (iconName == null) return;
    const ComponentName = toPascalCase(iconName);
    const iconNode = icons[ComponentName];
    if (!iconNode) {
      return console.warn(
        `${element.outerHTML} icon name was not found in the provided icons object.`
      );
    }
    const elementAttrs = getAttrs(element);
    const ariaProps = hasA11yProp(elementAttrs) ? {} : { "aria-hidden": "true" };
    const iconAttrs = {
      ...defaultAttributes,
      "data-lucide": iconName,
      ...ariaProps,
      ...attrs,
      ...elementAttrs
    };
    const elementClassNames = getClassNames(elementAttrs);
    const className = getClassNames(attrs);
    const classNames = mergeClasses(
      "lucide",
      `lucide-${iconName}`,
      ...elementClassNames,
      ...className
    );
    if (classNames) {
      Object.assign(iconAttrs, {
        class: classNames
      });
    }
    const svgElement = createElement(iconNode, iconAttrs);
    return element.parentNode?.replaceChild(svgElement, element);
  };

  // node_modules/lucide/dist/esm/icons/activity.mjs
  var Activity = [
    [
      "path",
      {
        d: "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"
      }
    ]
  ];

  // node_modules/lucide/dist/esm/icons/archive.mjs
  var Archive = [
    ["rect", { width: "20", height: "5", x: "2", y: "3", rx: "1" }],
    ["path", { d: "M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" }],
    ["path", { d: "M10 12h4" }]
  ];

  // node_modules/lucide/dist/esm/icons/check.mjs
  var Check = [["path", { d: "M20 6 9 17l-5-5" }]];

  // node_modules/lucide/dist/esm/icons/circle-x.mjs
  var CircleX = [
    ["circle", { cx: "12", cy: "12", r: "10" }],
    ["path", { d: "m15 9-6 6" }],
    ["path", { d: "m9 9 6 6" }]
  ];

  // node_modules/lucide/dist/esm/icons/database.mjs
  var Database = [
    ["ellipse", { cx: "12", cy: "5", rx: "9", ry: "3" }],
    ["path", { d: "M3 5V19A9 3 0 0 0 21 19V5" }],
    ["path", { d: "M3 12A9 3 0 0 0 21 12" }]
  ];

  // node_modules/lucide/dist/esm/icons/gauge.mjs
  var Gauge = [
    ["path", { d: "m12 14 4-4" }],
    ["path", { d: "M3.34 19a10 10 0 1 1 17.32 0" }]
  ];

  // node_modules/lucide/dist/esm/icons/heart-pulse.mjs
  var HeartPulse = [
    [
      "path",
      {
        d: "M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"
      }
    ],
    ["path", { d: "M3.22 13H9.5l.5-1 2 4.5 2-7 1.5 3.5h5.27" }]
  ];

  // node_modules/lucide/dist/esm/icons/refresh-cw.mjs
  var RefreshCw = [
    ["path", { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" }],
    ["path", { d: "M21 3v5h-5" }],
    ["path", { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" }],
    ["path", { d: "M8 16H3v5" }]
  ];

  // node_modules/lucide/dist/esm/icons/rotate-ccw.mjs
  var RotateCcw = [
    ["path", { d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" }],
    ["path", { d: "M3 3v5h5" }]
  ];

  // node_modules/lucide/dist/esm/icons/save.mjs
  var Save = [
    [
      "path",
      {
        d: "M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"
      }
    ],
    ["path", { d: "M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" }],
    ["path", { d: "M7 3v4a1 1 0 0 0 1 1h7" }]
  ];

  // node_modules/lucide/dist/esm/icons/settings-2.mjs
  var Settings2 = [
    ["path", { d: "M14 17H5" }],
    ["path", { d: "M19 7h-9" }],
    ["circle", { cx: "17", cy: "17", r: "3" }],
    ["circle", { cx: "7", cy: "7", r: "3" }]
  ];

  // node_modules/lucide/dist/esm/icons/shield-check.mjs
  var ShieldCheck = [
    [
      "path",
      {
        d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"
      }
    ],
    ["path", { d: "m9 12 2 2 4-4" }]
  ];

  // node_modules/lucide/dist/esm/icons/sliders-horizontal.mjs
  var SlidersHorizontal = [
    ["path", { d: "M10 5H3" }],
    ["path", { d: "M12 19H3" }],
    ["path", { d: "M14 3v4" }],
    ["path", { d: "M16 17v4" }],
    ["path", { d: "M21 12h-9" }],
    ["path", { d: "M21 19h-5" }],
    ["path", { d: "M21 5h-7" }],
    ["path", { d: "M8 10v4" }],
    ["path", { d: "M8 12H3" }]
  ];

  // node_modules/lucide/dist/esm/icons/x.mjs
  var X = [
    ["path", { d: "M18 6 6 18" }],
    ["path", { d: "m6 6 12 12" }]
  ];

  // node_modules/lucide/dist/esm/lucide.mjs
  var createIcons = ({
    icons = {},
    nameAttr = "data-lucide",
    attrs = {},
    root = document,
    inTemplates
  } = {}) => {
    if (!Object.values(icons).length) {
      throw new Error(
        "Please provide an icons object.\nIf you want to use all the icons you can import it like:\n `import { createIcons, icons } from 'lucide';\nlucide.createIcons({icons});`"
      );
    }
    if (typeof root === "undefined") {
      throw new Error("`createIcons()` only works in a browser environment.");
    }
    const elementsToReplace = Array.from(root.querySelectorAll(`[${nameAttr}]`));
    elementsToReplace.forEach((element) => replaceElement(element, { nameAttr, icons, attrs }));
    if (inTemplates) {
      const templates = Array.from(root.querySelectorAll("template"));
      templates.forEach(
        (template) => createIcons({
          icons,
          nameAttr,
          attrs,
          root: template.content,
          inTemplates
        })
      );
    }
    if (nameAttr === "data-lucide") {
      const deprecatedElements = root.querySelectorAll("[icon-name]");
      if (deprecatedElements.length > 0) {
        console.warn(
          "[Lucide] Some icons were found with the now deprecated icon-name attribute. These will still be replaced for backwards compatibility, but will no longer be supported in v1.0 and you should switch to data-lucide"
        );
        Array.from(deprecatedElements).forEach(
          (element) => replaceElement(element, { nameAttr: "icon-name", icons, attrs })
        );
      }
    }
  };

  // ui/settings/generated/validate-local-runtime-config-v2.js
  var validate_local_runtime_config_v2_default = validate20;
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
        if (data.schema_version === void 0 && (missing0 = "schema_version") || data.enabled === void 0 && (missing0 = "enabled") || data.collection === void 0 && (missing0 = "collection") || data.retention === void 0 && (missing0 = "retention")) {
          validate20.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          const _errs1 = errors;
          for (const key0 in data) {
            if (!(key0 === "schema_version" || key0 === "enabled" || key0 === "collection" || key0 === "retention")) {
              validate20.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
              return false;
              break;
            }
          }
          if (_errs1 === errors) {
            if (data.schema_version !== void 0) {
              const _errs2 = errors;
              if ("local_runtime.v2" !== data.schema_version) {
                validate20.errors = [{ instancePath: instancePath + "/schema_version", schemaPath: "#/properties/schema_version/const", keyword: "const", params: { allowedValue: "local_runtime.v2" }, message: "must be equal to constant" }];
                return false;
              }
              var valid0 = _errs2 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.enabled !== void 0) {
                const _errs3 = errors;
                if (typeof data.enabled !== "boolean") {
                  validate20.errors = [{ instancePath: instancePath + "/enabled", schemaPath: "#/properties/enabled/type", keyword: "type", params: { type: "boolean" }, message: "must be boolean" }];
                  return false;
                }
                var valid0 = _errs3 === errors;
              } else {
                var valid0 = true;
              }
              if (valid0) {
                if (data.collection !== void 0) {
                  let data2 = data.collection;
                  const _errs5 = errors;
                  const _errs6 = errors;
                  if (errors === _errs6) {
                    if (data2 && typeof data2 == "object" && !Array.isArray(data2)) {
                      let missing1;
                      if (data2.file_reconcile_interval_ms === void 0 && (missing1 = "file_reconcile_interval_ms") || data2.flush_interval_ms === void 0 && (missing1 = "flush_interval_ms") || data2.max_batch_records === void 0 && (missing1 = "max_batch_records") || data2.max_batch_bytes === void 0 && (missing1 = "max_batch_bytes") || data2.active_heartbeat_interval_ms === void 0 && (missing1 = "active_heartbeat_interval_ms") || data2.idle_heartbeat_interval_ms === void 0 && (missing1 = "idle_heartbeat_interval_ms") || data2.local_storage_budget_bytes === void 0 && (missing1 = "local_storage_budget_bytes")) {
                        validate20.errors = [{ instancePath: instancePath + "/collection", schemaPath: "#/$defs/collection/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                        return false;
                      } else {
                        const _errs8 = errors;
                        for (const key1 in data2) {
                          if (!(key1 === "file_reconcile_interval_ms" || key1 === "flush_interval_ms" || key1 === "max_batch_records" || key1 === "max_batch_bytes" || key1 === "active_heartbeat_interval_ms" || key1 === "idle_heartbeat_interval_ms" || key1 === "local_storage_budget_bytes")) {
                            validate20.errors = [{ instancePath: instancePath + "/collection", schemaPath: "#/$defs/collection/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                            return false;
                            break;
                          }
                        }
                        if (_errs8 === errors) {
                          if (data2.file_reconcile_interval_ms !== void 0) {
                            let data3 = data2.file_reconcile_interval_ms;
                            const _errs9 = errors;
                            if (!(typeof data3 == "number" && (!(data3 % 1) && !isNaN(data3)) && isFinite(data3))) {
                              validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                              return false;
                            }
                            if (errors === _errs9) {
                              if (typeof data3 == "number" && isFinite(data3)) {
                                if (data3 > 6e4 || isNaN(data3)) {
                                  validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 6e4 }, message: "must be <= 60000" }];
                                  return false;
                                } else {
                                  if (data3 < 1e3 || isNaN(data3)) {
                                    validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1e3 }, message: "must be >= 1000" }];
                                    return false;
                                  }
                                }
                              }
                            }
                            var valid2 = _errs9 === errors;
                          } else {
                            var valid2 = true;
                          }
                          if (valid2) {
                            if (data2.flush_interval_ms !== void 0) {
                              let data4 = data2.flush_interval_ms;
                              const _errs11 = errors;
                              if (!(typeof data4 == "number" && (!(data4 % 1) && !isNaN(data4)) && isFinite(data4))) {
                                validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                return false;
                              }
                              if (errors === _errs11) {
                                if (typeof data4 == "number" && isFinite(data4)) {
                                  if (data4 > 6e4 || isNaN(data4)) {
                                    validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 6e4 }, message: "must be <= 60000" }];
                                    return false;
                                  } else {
                                    if (data4 < 1e3 || isNaN(data4)) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1e3 }, message: "must be >= 1000" }];
                                      return false;
                                    }
                                  }
                                }
                              }
                              var valid2 = _errs11 === errors;
                            } else {
                              var valid2 = true;
                            }
                            if (valid2) {
                              if (data2.max_batch_records !== void 0) {
                                let data5 = data2.max_batch_records;
                                const _errs13 = errors;
                                if (!(typeof data5 == "number" && (!(data5 % 1) && !isNaN(data5)) && isFinite(data5))) {
                                  validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                  return false;
                                }
                                if (errors === _errs13) {
                                  if (typeof data5 == "number" && isFinite(data5)) {
                                    if (data5 > 500 || isNaN(data5)) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/maximum", keyword: "maximum", params: { comparison: "<=", limit: 500 }, message: "must be <= 500" }];
                                      return false;
                                    } else {
                                      if (data5 < 1 || isNaN(data5)) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                        return false;
                                      }
                                    }
                                  }
                                }
                                var valid2 = _errs13 === errors;
                              } else {
                                var valid2 = true;
                              }
                              if (valid2) {
                                if (data2.max_batch_bytes !== void 0) {
                                  let data6 = data2.max_batch_bytes;
                                  const _errs15 = errors;
                                  if (!(typeof data6 == "number" && (!(data6 % 1) && !isNaN(data6)) && isFinite(data6))) {
                                    validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs15) {
                                    if (typeof data6 == "number" && isFinite(data6)) {
                                      if (data6 > 2097152 || isNaN(data6)) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 2097152 }, message: "must be <= 2097152" }];
                                        return false;
                                      } else {
                                        if (data6 < 16384 || isNaN(data6)) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 16384 }, message: "must be >= 16384" }];
                                          return false;
                                        }
                                      }
                                    }
                                  }
                                  var valid2 = _errs15 === errors;
                                } else {
                                  var valid2 = true;
                                }
                                if (valid2) {
                                  if (data2.active_heartbeat_interval_ms !== void 0) {
                                    let data7 = data2.active_heartbeat_interval_ms;
                                    const _errs17 = errors;
                                    if (!(typeof data7 == "number" && (!(data7 % 1) && !isNaN(data7)) && isFinite(data7))) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                      return false;
                                    }
                                    if (errors === _errs17) {
                                      if (typeof data7 == "number" && isFinite(data7)) {
                                        if (data7 > 3e5 || isNaN(data7)) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3e5 }, message: "must be <= 300000" }];
                                          return false;
                                        } else {
                                          if (data7 < 3e4 || isNaN(data7)) {
                                            validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 3e4 }, message: "must be >= 30000" }];
                                            return false;
                                          }
                                        }
                                      }
                                    }
                                    var valid2 = _errs17 === errors;
                                  } else {
                                    var valid2 = true;
                                  }
                                  if (valid2) {
                                    if (data2.idle_heartbeat_interval_ms !== void 0) {
                                      let data8 = data2.idle_heartbeat_interval_ms;
                                      const _errs19 = errors;
                                      if (!(typeof data8 == "number" && (!(data8 % 1) && !isNaN(data8)) && isFinite(data8))) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                        return false;
                                      }
                                      if (errors === _errs19) {
                                        if (typeof data8 == "number" && isFinite(data8)) {
                                          if (data8 > 9e5 || isNaN(data8)) {
                                            validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9e5 }, message: "must be <= 900000" }];
                                            return false;
                                          } else {
                                            if (data8 < 12e4 || isNaN(data8)) {
                                              validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 12e4 }, message: "must be >= 120000" }];
                                              return false;
                                            }
                                          }
                                        }
                                      }
                                      var valid2 = _errs19 === errors;
                                    } else {
                                      var valid2 = true;
                                    }
                                    if (valid2) {
                                      if (data2.local_storage_budget_bytes !== void 0) {
                                        let data9 = data2.local_storage_budget_bytes;
                                        const _errs21 = errors;
                                        if (!(typeof data9 == "number" && (!(data9 % 1) && !isNaN(data9)) && isFinite(data9))) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                          return false;
                                        }
                                        if (errors === _errs21) {
                                          if (typeof data9 == "number" && isFinite(data9)) {
                                            if (data9 > 21474836480 || isNaN(data9)) {
                                              validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 21474836480 }, message: "must be <= 21474836480" }];
                                              return false;
                                            } else {
                                              if (data9 < 268435456 || isNaN(data9)) {
                                                validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 268435456 }, message: "must be >= 268435456" }];
                                                return false;
                                              }
                                            }
                                          }
                                        }
                                        var valid2 = _errs21 === errors;
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
                    } else {
                      validate20.errors = [{ instancePath: instancePath + "/collection", schemaPath: "#/$defs/collection/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                      return false;
                    }
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.retention !== void 0) {
                    let data10 = data.retention;
                    const _errs23 = errors;
                    const _errs24 = errors;
                    if (errors === _errs24) {
                      if (data10 && typeof data10 == "object" && !Array.isArray(data10)) {
                        let missing2;
                        if (data10.max_record_age_days === void 0 && (missing2 = "max_record_age_days") || data10.max_archive_records === void 0 && (missing2 = "max_archive_records") || data10.max_archive_bytes === void 0 && (missing2 = "max_archive_bytes")) {
                          validate20.errors = [{ instancePath: instancePath + "/retention", schemaPath: "#/$defs/retention/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                          return false;
                        } else {
                          const _errs26 = errors;
                          for (const key2 in data10) {
                            if (!(key2 === "max_record_age_days" || key2 === "max_archive_records" || key2 === "max_archive_bytes")) {
                              validate20.errors = [{ instancePath: instancePath + "/retention", schemaPath: "#/$defs/retention/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs26 === errors) {
                            if (data10.max_record_age_days !== void 0) {
                              let data11 = data10.max_record_age_days;
                              const _errs27 = errors;
                              if (!(typeof data11 == "number" && (!(data11 % 1) && !isNaN(data11)) && isFinite(data11))) {
                                validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                return false;
                              }
                              if (errors === _errs27) {
                                if (typeof data11 == "number" && isFinite(data11)) {
                                  if (data11 > 3650 || isNaN(data11)) {
                                    validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                    return false;
                                  } else {
                                    if (data11 < 1 || isNaN(data11)) {
                                      validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                      return false;
                                    }
                                  }
                                }
                              }
                              var valid4 = _errs27 === errors;
                            } else {
                              var valid4 = true;
                            }
                            if (valid4) {
                              if (data10.max_archive_records !== void 0) {
                                let data12 = data10.max_archive_records;
                                const _errs29 = errors;
                                if (!(typeof data12 == "number" && (!(data12 % 1) && !isNaN(data12)) && isFinite(data12))) {
                                  validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                  return false;
                                }
                                if (errors === _errs29) {
                                  if (typeof data12 == "number" && isFinite(data12)) {
                                    if (data12 > 1e5 || isNaN(data12)) {
                                      validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/maximum", keyword: "maximum", params: { comparison: "<=", limit: 1e5 }, message: "must be <= 100000" }];
                                      return false;
                                    } else {
                                      if (data12 < 1 || isNaN(data12)) {
                                        validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                        return false;
                                      }
                                    }
                                  }
                                }
                                var valid4 = _errs29 === errors;
                              } else {
                                var valid4 = true;
                              }
                              if (valid4) {
                                if (data10.max_archive_bytes !== void 0) {
                                  let data13 = data10.max_archive_bytes;
                                  const _errs31 = errors;
                                  if (!(typeof data13 == "number" && (!(data13 % 1) && !isNaN(data13)) && isFinite(data13))) {
                                    validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs31) {
                                    if (typeof data13 == "number" && isFinite(data13)) {
                                      if (data13 > 268435456 || isNaN(data13)) {
                                        validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 268435456 }, message: "must be <= 268435456" }];
                                        return false;
                                      } else {
                                        if (data13 < 65536 || isNaN(data13)) {
                                          validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 65536 }, message: "must be >= 65536" }];
                                          return false;
                                        }
                                      }
                                    }
                                  }
                                  var valid4 = _errs31 === errors;
                                } else {
                                  var valid4 = true;
                                }
                              }
                            }
                          }
                        }
                      } else {
                        validate20.errors = [{ instancePath: instancePath + "/retention", schemaPath: "#/$defs/retention/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                        return false;
                      }
                    }
                    var valid0 = _errs23 === errors;
                  } else {
                    var valid0 = true;
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

  // ui/settings/main.ts
  var fields = {
    "collection.file_reconcile_interval_ms": {
      path: "collection.file_reconcile_interval_ms",
      label: "\uD30C\uC77C \uD655\uC778 \uC8FC\uAE30",
      description: "\uC0C8 handoff \uD30C\uC77C\uC744 \uB2E4\uC2DC \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 1e3,
      max: 6e4,
      step: 1e3,
      unit: "ms",
      format: formatDuration
    },
    "collection.flush_interval_ms": {
      path: "collection.flush_interval_ms",
      label: "\uAE30\uB85D \uBC18\uC601 \uC8FC\uAE30",
      description: "\uD5C8\uC6A9\uB41C \uBC30\uCE58\uB97C durable storage\uC5D0 \uBC18\uC601\uD558\uB294 \uAC04\uACA9",
      min: 1e3,
      max: 6e4,
      step: 1e3,
      unit: "ms",
      format: formatDuration
    },
    "collection.max_batch_records": {
      path: "collection.max_batch_records",
      label: "\uBC30\uCE58 \uB808\uCF54\uB4DC",
      description: "\uD55C \uBC88\uC5D0 \uCC98\uB9AC\uD560 \uCD5C\uB300 \uB808\uCF54\uB4DC \uC218",
      min: 1,
      max: 500,
      step: 1,
      unit: "records",
      format: (value) => `${formatNumber(value)}\uAC1C`
    },
    "collection.max_batch_bytes": {
      path: "collection.max_batch_bytes",
      label: "\uBC30\uCE58 \uD06C\uAE30",
      description: "\uD55C \uBC88\uC5D0 \uCC98\uB9AC\uD560 \uCD5C\uB300 byte \uD06C\uAE30",
      min: 16384,
      max: 2097152,
      step: 16384,
      unit: "bytes",
      format: formatBytes
    },
    "collection.active_heartbeat_interval_ms": {
      path: "collection.active_heartbeat_interval_ms",
      label: "\uD65C\uC131 heartbeat",
      description: "\uC791\uC5C5 \uC911 source \uC0C1\uD0DC\uB97C \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 3e4,
      max: 3e5,
      step: 1e4,
      unit: "ms",
      format: formatDuration
    },
    "collection.idle_heartbeat_interval_ms": {
      path: "collection.idle_heartbeat_interval_ms",
      label: "\uC720\uD734 heartbeat",
      description: "\uC791\uC5C5\uC774 \uC5C6\uC744 \uB54C source \uC0C1\uD0DC\uB97C \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 12e4,
      max: 9e5,
      step: 3e4,
      unit: "ms",
      format: formatDuration
    },
    "collection.local_storage_budget_bytes": {
      path: "collection.local_storage_budget_bytes",
      label: "\uB85C\uCEEC \uC800\uC7A5 \uD55C\uB3C4",
      description: "\uC218\uC9D1 \uB370\uC774\uD130\uAC00 \uC0AC\uC6A9\uD560 \uC218 \uC788\uB294 \uCD5C\uB300 \uB514\uC2A4\uD06C \uC608\uC0B0",
      min: 268435456,
      max: 21474836480,
      step: 268435456,
      unit: "bytes",
      format: formatBytes
    },
    "retention.max_record_age_days": {
      path: "retention.max_record_age_days",
      label: "\uBCF4\uAD00 \uAE30\uAC04",
      description: "\uC774 \uAE30\uAC04\uBCF4\uB2E4 \uC624\uB798\uB41C trace\uB294 \uB9CC\uB8CC \uB300\uC0C1",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "retention.max_archive_records": {
      path: "retention.max_archive_records",
      label: "archive \uB808\uCF54\uB4DC",
      description: "\uD558\uB098\uC758 private archive\uC5D0 \uB2F4\uC744 \uCD5C\uB300 \uB808\uCF54\uB4DC \uC218",
      min: 1,
      max: 1e5,
      step: 1,
      unit: "records",
      format: (value) => `${formatNumber(value)}\uAC1C`
    },
    "retention.max_archive_bytes": {
      path: "retention.max_archive_bytes",
      label: "archive \uD06C\uAE30",
      description: "\uD558\uB098\uC758 private archive\uC5D0 \uB2F4\uC744 \uCD5C\uB300 \uD06C\uAE30",
      min: 65536,
      max: 268435456,
      step: 65536,
      unit: "bytes",
      format: formatBytes
    }
  };
  var rootElement = document.querySelector("#app");
  if (!(rootElement instanceof HTMLDivElement)) throw new Error("settings root is missing");
  var app = rootElement;
  var token = new URLSearchParams(location.hash.slice(1)).get("session") ?? "";
  history.replaceState(null, "", `${location.pathname}${location.search}`);
  var persisted = null;
  var draft = null;
  var defaults = null;
  var revision = "";
  var busy = false;
  var conflicted = false;
  var heartbeatTimer;
  var navigationObserver;
  var lastUserActivity = Date.now();
  for (const eventName of ["pointerdown", "keydown", "input", "scroll"]) {
    document.addEventListener(eventName, () => {
      lastUserActivity = Date.now();
    }, { passive: true });
  }
  void bootstrap();
  async function bootstrap() {
    renderLoading();
    if (!token) {
      renderExpired();
      return;
    }
    try {
      applyEnvelope(await api("/api/config"));
      renderSettings();
      heartbeatTimer = window.setInterval(() => void heartbeat(), 2e4);
    } catch (error) {
      renderUnavailable(messageOf(error));
    }
  }
  function renderLoading() {
    app.innerHTML = `<main class="center-state" aria-busy="true">
    <i data-lucide="settings-2" aria-hidden="true"></i>
    <h1>\uB85C\uCEEC \uC124\uC815\uC744 \uBD88\uB7EC\uC624\uB294 \uC911</h1>
    <p>Rust runtime\uC758 \uD604\uC7AC \uC815\uCC45\uC744 \uD655\uC778\uD558\uACE0 \uC788\uC2B5\uB2C8\uB2E4.</p>
  </main>`;
    mountIcons();
  }
  function renderUnavailable(message) {
    app.innerHTML = `<main class="center-state" role="alert">
    <i data-lucide="x-circle" aria-hidden="true"></i>
    <h1>\uC124\uC815\uC744 \uBD88\uB7EC\uC624\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4</h1>
    <p id="fatal-message"></p>
    <button class="button primary" id="retry"><i data-lucide="refresh-cw"></i>\uB2E4\uC2DC \uC2DC\uB3C4</button>
  </main>`;
    setText("fatal-message", message);
    document.querySelector("#retry")?.addEventListener("click", () => void bootstrap());
    mountIcons();
  }
  function renderExpired() {
    window.clearInterval(heartbeatTimer);
    app.innerHTML = `<main class="center-state" role="alert">
    <i data-lucide="shield-check" aria-hidden="true"></i>
    <h1>\uC124\uC815 \uC138\uC158\uC774 \uC885\uB8CC\uB418\uC5C8\uC2B5\uB2C8\uB2E4</h1>
    <p>\uD130\uBBF8\uB110\uC5D0\uC11C <code>agent-observability ui</code>\uB97C \uC2E4\uD589\uD574 \uC0C8 \uC138\uC158\uC744 \uC5EC\uC138\uC694.</p>
  </main>`;
    mountIcons();
  }
  function renderSettings(focusTarget) {
    if (!draft) return;
    app.innerHTML = `<div class="app-shell">
    <header class="topbar">
      <div class="brand"><span class="brand-mark"><i data-lucide="settings-2"></i></span><span>Agent Observability</span></div>
      <div class="topbar-actions">
        <span class="session-badge"><i data-lucide="shield-check"></i>\uB85C\uCEEC \uC804\uC6A9 \xB7 \uC138\uC158 \uD65C\uC131</span>
        <button class="icon-button" id="close-session" type="button" title="\uC124\uC815 \uC138\uC158 \uB2EB\uAE30" aria-label="\uC124\uC815 \uC138\uC158 \uB2EB\uAE30"><i data-lucide="x"></i></button>
      </div>
    </header>
    <div class="workspace">
      <nav class="section-nav" aria-label="\uC124\uC815 \uC601\uC5ED">
        <p class="nav-label">\uC124\uC815</p>
        <a href="#overview" class="active" aria-current="page"><i data-lucide="gauge"></i>\uAC1C\uC694</a>
        <a href="#collection"><i data-lucide="activity"></i>\uC218\uC9D1</a>
        <a href="#storage"><i data-lucide="database"></i>\uC800\uC7A5\uC18C</a>
        <a href="#retention"><i data-lucide="archive"></i>\uBCF4\uAD00</a>
        <div class="nav-note"><strong>\uC785\uB825 \uBC29\uC2DD</strong><span>\uC218\uB3D9 private handoff</span><span>\uC790\uB3D9 producer \uBBF8\uD3EC\uD568</span></div>
      </nav>
      <main class="settings-main">
        <form id="settings-form" novalidate>
          ${overviewSection(draft)}
          ${collectionSection(draft)}
          ${storageSection(draft)}
          ${retentionSection(draft)}
        </form>
      </main>
    </div>
    <div class="save-band" id="save-band">
      <div class="save-state"><span class="state-dot"></span><strong id="save-title" tabindex="-1">\uC800\uC7A5\uB428</strong><span id="save-detail">\uD604\uC7AC \uC124\uC815\uACFC \uAC19\uC2B5\uB2C8\uB2E4.</span></div>
      <div class="save-actions">
        <button class="button ghost" id="discard" type="button" disabled>\uBCC0\uACBD \uCDE8\uC18C</button>
        <button class="button secondary" id="reset" type="button"><i data-lucide="rotate-ccw"></i>\uAE30\uBCF8\uAC12</button>
        <button class="button primary" id="save" type="submit" form="settings-form" disabled><i data-lucide="save"></i>\uC124\uC815 \uC800\uC7A5</button>
      </div>
    </div>
    <div class="toast" id="toast" role="status" aria-live="polite"></div>
    <dialog id="reset-dialog" aria-labelledby="reset-title">
      <div class="dialog-heading"><i data-lucide="rotate-ccw"></i><div><h2 id="reset-title">\uAE30\uBCF8\uAC12\uC73C\uB85C \uBCF5\uC6D0</h2><p>\uC218\uC9D1, \uC800\uC7A5\uC18C, \uBCF4\uAD00 \uC815\uCC45\uC758 \uD3B8\uC9D1\uAC12\uC744 \uCD08\uAE30\uAC12\uC73C\uB85C \uBC14\uAFC9\uB2C8\uB2E4.</p></div></div>
      <div class="dialog-actions"><button class="button ghost" id="cancel-reset" type="button">\uCDE8\uC18C</button><button class="button primary" id="confirm-reset" type="button">\uD3B8\uC9D1\uAC12 \uBCF5\uC6D0</button></div>
    </dialog>
    <dialog id="close-dialog" aria-labelledby="close-title">
      <div class="dialog-heading"><i data-lucide="x-circle"></i><div><h2 id="close-title">\uC800\uC7A5\uD558\uC9C0 \uC54A\uC740 \uBCC0\uACBD \uB2EB\uAE30</h2><p>\uD604\uC7AC \uD3B8\uC9D1\uAC12\uC740 \uC800\uC7A5\uB418\uC9C0 \uC54A\uC558\uC2B5\uB2C8\uB2E4. \uC124\uC815 \uC138\uC158\uC744 \uC885\uB8CC\uD558\uBA74 \uBCC0\uACBD\uC744 \uC783\uC2B5\uB2C8\uB2E4.</p></div></div>
      <div class="dialog-actions"><button class="button ghost" id="cancel-close" type="button">\uACC4\uC18D \uD3B8\uC9D1</button><button class="button danger" id="confirm-close" type="button">\uBCC0\uACBD \uBC84\uB9AC\uACE0 \uB2EB\uAE30</button></div>
    </dialog>
  </div>`;
    bindEvents();
    updateAllVisuals();
    updateDirtyState();
    mountIcons();
    if (focusTarget) {
      requestAnimationFrame(() => document.querySelector(`#${focusTarget}`)?.focus());
    }
  }
  function overviewSection(config) {
    const storage = fields["collection.local_storage_budget_bytes"].format(
      config.collection.local_storage_budget_bytes
    );
    return `<section class="settings-section overview" id="overview" aria-labelledby="overview-title">
    <div class="section-heading"><div><p class="eyebrow">Standalone</p><h1 id="overview-title">\uB85C\uCEEC \uC218\uC9D1 \uC815\uCC45</h1><p>\uC815\uC801 \uB9AC\uD3EC\uD2B8\uC640 \uB3C5\uB9BD\uC801\uC73C\uB85C \uC800\uC7A5\xB7\uBCF4\uAD00 \uD55C\uB3C4\uB97C \uAD00\uB9AC\uD569\uB2C8\uB2E4.</p></div>
      <label class="collection-toggle"><span><strong>\uC218\uC9D1 \uD5C8\uC6A9</strong><small id="enabled-copy">${config.enabled ? "private handoff\uB97C \uCC98\uB9AC\uD569\uB2C8\uB2E4" : "\uC124\uC815\uAC12\uC744 \uC720\uC9C0\uD55C \uCC44 \uCC98\uB9AC\uB97C \uC911\uC9C0\uD569\uB2C8\uB2E4"}</small></span><input type="checkbox" id="enabled" ${config.enabled ? "checked" : ""}><span class="toggle-track" aria-hidden="true"><span></span></span></label>
    </div>
    <div class="policy-strip" aria-label="\uC815\uCC45 \uC694\uC57D">
      ${summaryItem("activity", "\uD655\uC778 \uC8FC\uAE30", formatDuration(config.collection.file_reconcile_interval_ms))}
      ${summaryItem("sliders-horizontal", "\uBC30\uCE58 \uC0C1\uD55C", `${formatNumber(config.collection.max_batch_records)}\uAC1C`)}
      ${summaryItem("database", "\uC800\uC7A5 \uD55C\uB3C4", storage)}
      ${summaryItem("archive", "\uBCF4\uAD00 \uAE30\uAC04", `${formatNumber(config.retention.max_record_age_days)}\uC77C`)}
    </div>
    <div class="policy-notice"><i data-lucide="shield-check"></i><div><strong>\uC774 \uD654\uBA74\uC740 \uB85C\uCEEC \uC815\uCC45\uB9CC \uBCC0\uACBD\uD569\uB2C8\uB2E4.</strong><span>\uC678\uBD80 \uC804\uC1A1 \uC5C6\uC774 Rust\uAC00 \uAC80\uC99D\uD55C \uB4A4 private config\uC5D0 \uC6D0\uC790\uC801\uC73C\uB85C \uC800\uC7A5\uD569\uB2C8\uB2E4.</span></div></div>
  </section>`;
  }
  function collectionSection(config) {
    return `<section class="settings-section" id="collection" aria-labelledby="collection-title">
    ${sectionTitle("activity", "\uC218\uC9D1", "\uD30C\uC77C \uD655\uC778\uACFC durable \uAE30\uB85D \uBC18\uC601 \uAC04\uACA9")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["collection.file_reconcile_interval_ms"], config)}${fieldControl(fields["collection.flush_interval_ms"], config)}</div>
      ${dualTimeline(
      "cadence-visual",
      "\uC218\uC9D1 cadence",
      "\uD655\uC778",
      fields["collection.file_reconcile_interval_ms"],
      "\uBC18\uC601",
      fields["collection.flush_interval_ms"],
      "1\uCD08",
      "60\uCD08"
    )}
    </div>
    <div class="subsection">
      <div class="subsection-heading"><h3>\uBC30\uCE58 \uBC0F \uC0C1\uD0DC \uD655\uC778</h3><p>\uCC98\uB9AC\uB7C9\uACFC source \uC0C1\uD0DC \uD655\uC778 \uAC04\uACA9\uC744 bounded policy\uB85C \uC81C\uD55C\uD569\uB2C8\uB2E4.</p></div>
      <div class="section-grid">
        <div class="field-grid">${fieldControl(fields["collection.max_batch_records"], config)}${fieldControl(fields["collection.max_batch_bytes"], config)}${fieldControl(fields["collection.active_heartbeat_interval_ms"], config)}${fieldControl(fields["collection.idle_heartbeat_interval_ms"], config)}</div>
        <div class="visual-stack">
          ${singleRuler("batch-records-visual", "\uBC30\uCE58 \uB808\uCF54\uB4DC \uC0C1\uD55C", fields["collection.max_batch_records"], "1", "500")}
          ${singleRuler("batch-bytes-visual", "\uBC30\uCE58 \uD06C\uAE30 \uC0C1\uD55C", fields["collection.max_batch_bytes"], "16 KiB", "2 MiB")}
          ${dualTimeline("heartbeat-visual", "Heartbeat \uAC04\uACA9", "\uD65C\uC131", fields["collection.active_heartbeat_interval_ms"], "\uC720\uD734", fields["collection.idle_heartbeat_interval_ms"], "30\uCD08", "15\uBD84", true, 3e4, 9e5)}
        </div>
      </div>
    </div>
  </section>`;
  }
  function storageSection(config) {
    return `<section class="settings-section" id="storage" aria-labelledby="storage-title">
    ${sectionTitle("database", "\uC800\uC7A5\uC18C", "\uB85C\uCEEC \uB370\uC774\uD130\uAC00 \uB118\uC9C0 \uBABB\uD558\uB294 \uB514\uC2A4\uD06C \uC608\uC0B0")}
    <div class="section-grid">
      <div class="field-grid single">${fieldControl(fields["collection.local_storage_budget_bytes"], config)}</div>
      ${singleRuler("storage-visual", "\uC124\uC815 \uC800\uC7A5 \uD55C\uB3C4", fields["collection.local_storage_budget_bytes"], "256 MiB", "20 GiB", true, "\uD604\uC7AC \uC0AC\uC6A9\uB7C9\uC774 \uC544\uB2CC \uD5C8\uC6A9 \uD55C\uB3C4")}
    </div>
  </section>`;
  }
  function retentionSection(config) {
    return `<section class="settings-section" id="retention" aria-labelledby="retention-title">
    ${sectionTitle("archive", "\uBCF4\uAD00", "\uB9CC\uB8CC \uB300\uC0C1\uACFC private archive \uD06C\uAE30 \uC815\uCC45")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["retention.max_record_age_days"], config)}${fieldControl(fields["retention.max_archive_records"], config)}${fieldControl(fields["retention.max_archive_bytes"], config)}</div>
      <div class="visual-stack">
        ${singleRuler("retention-visual", "\uBCF4\uAD00 \uAE30\uAC04", fields["retention.max_record_age_days"], "1\uC77C", "10\uB144", true, "cutoff\uBCF4\uB2E4 \uC624\uB798\uB41C trace\uB294 \uB9CC\uB8CC \uB300\uC0C1")}
        ${singleRuler("archive-records-visual", "Archive \uB808\uCF54\uB4DC \uC0C1\uD55C", fields["retention.max_archive_records"], "1", "100k", true)}
        ${singleRuler("archive-bytes-visual", "Archive \uD06C\uAE30 \uC0C1\uD55C", fields["retention.max_archive_bytes"], "64 KiB", "256 MiB", true)}
      </div>
    </div>
    <div class="retention-note"><i data-lucide="archive"></i><span>\uBCF4\uAD00 \uAE30\uAC04\uC744 \uC904\uC5EC\uB3C4 \uC989\uC2DC \uC0AD\uC81C\uD558\uC9C0 \uC54A\uC2B5\uB2C8\uB2E4. cleanup\uC740 \uBCC4\uB3C4\uC758 retention plan/apply \uACBD\uACC4\uB97C \uB530\uB985\uB2C8\uB2E4.</span></div>
  </section>`;
  }
  function sectionTitle(icon, title, description) {
    return `<div class="section-title"><span class="section-icon"><i data-lucide="${icon}"></i></span><div><h2 id="${title === "\uC218\uC9D1" ? "collection" : title === "\uC800\uC7A5\uC18C" ? "storage" : "retention"}-title">${title}</h2><p>${description}</p></div></div>`;
  }
  function summaryItem(icon, label, value) {
    return `<div class="summary-item"><i data-lucide="${icon}"></i><span>${label}</span><strong>${value}</strong></div>`;
  }
  function fieldControl(field, config) {
    const value = getValue(config, field.path);
    const id = field.path.replaceAll(".", "-");
    return `<div class="field" data-field="${field.path}">
    <label for="${id}">${field.label}<span class="changed-label" aria-hidden="true">\uBCC0\uACBD\uB428</span></label>
    <p id="${id}-help">${field.description}</p>
    <div class="number-control"><input id="${id}" name="${field.path}" data-path="${field.path}" type="number" value="${value}" min="${field.min}" max="${field.max}" step="${field.step}" inputmode="numeric" required aria-describedby="${id}-help ${id}-readout"><span>${field.unit}</span></div>
    <output id="${id}-readout" for="${id}">${field.format(value)}</output>
    <span class="field-error" id="${id}-error"></span>
  </div>`;
  }
  function singleRuler(id, title, field, min, max, logarithmic = false, caption = "\uC124\uC815\uB41C \uC815\uCC45 \uC0C1\uD55C") {
    return `<figure class="policy-visual" id="${id}" data-path="${field.path}" data-log="${logarithmic}">
    <figcaption><span>${title}</span><strong data-visual-value></strong></figcaption>
    <div class="ruler" aria-hidden="true"><span class="ruler-marker" data-marker></span></div>
    <div class="ruler-labels"><span>${min}</span><span>${max}</span></div>
    <p>${caption}</p>
  </figure>`;
  }
  function dualTimeline(id, title, firstLabel, first, secondLabel, second, min, max, logarithmic = false, sharedMin, sharedMax) {
    const sharedScale = sharedMin === void 0 || sharedMax === void 0 ? "" : ` data-min="${sharedMin}" data-max="${sharedMax}"`;
    return `<figure class="policy-visual timeline" id="${id}" data-log="${logarithmic}"${sharedScale}>
    <figcaption><span>${title}</span><strong data-dual-value></strong></figcaption>
    <div class="timeline-track" aria-hidden="true">
      <span class="timeline-marker first" data-marker data-path="${first.path}"><b>${firstLabel}</b></span>
      <span class="timeline-marker second" data-marker data-path="${second.path}"><b>${secondLabel}</b></span>
    </div>
    <div class="ruler-labels"><span>${min}</span><span>${max}</span></div>
    <p>\uAC01 marker\uB294 \uC124\uC815 \uAC04\uACA9\uC774\uBA70 \uC2E4\uC2DC\uAC04 \uCC98\uB9AC\uB7C9\uC774 \uC544\uB2D9\uB2C8\uB2E4.</p>
  </figure>`;
  }
  function bindEvents() {
    const form = document.querySelector("#settings-form");
    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      void saveDraft();
    });
    form?.addEventListener("input", handleInput);
    document.querySelector("#enabled")?.addEventListener("change", handleEnabled);
    document.querySelector("#discard")?.addEventListener("click", discardChanges);
    document.querySelector("#reset")?.addEventListener("click", openResetDialog);
    document.querySelector("#cancel-reset")?.addEventListener("click", closeResetDialog);
    document.querySelector("#confirm-reset")?.addEventListener("click", resetDefaults);
    document.querySelector("#close-session")?.addEventListener("click", requestCloseSession);
    document.querySelector("#cancel-close")?.addEventListener("click", closeCloseDialog);
    document.querySelector("#confirm-close")?.addEventListener("click", () => void closeSession());
    document.querySelectorAll(".section-nav a").forEach((link) => {
      link.addEventListener("click", () => {
        setActiveNavigation(link.hash);
      });
    });
    navigationObserver?.disconnect();
    navigationObserver = new IntersectionObserver(
      (entries) => {
        const visible = entries.find((entry) => entry.isIntersecting);
        if (visible) setActiveNavigation(`#${visible.target.id}`);
      },
      { rootMargin: "-32% 0px -60% 0px", threshold: 0 }
    );
    document.querySelectorAll(".settings-section").forEach((section) => navigationObserver?.observe(section));
  }
  function handleInput(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || !draft) return;
    const path = input.dataset.path;
    if (!path) return;
    const value = Number(input.value);
    if (Number.isFinite(value)) setValue(draft, path, value);
    clearFieldError(path);
    updateAllVisuals();
    updateDirtyState();
  }
  function handleEnabled(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || !draft) return;
    draft.enabled = input.checked;
    setText(
      "enabled-copy",
      input.checked ? "private handoff\uB97C \uCC98\uB9AC\uD569\uB2C8\uB2E4" : "\uC124\uC815\uAC12\uC744 \uC720\uC9C0\uD55C \uCC44 \uCC98\uB9AC\uB97C \uC911\uC9C0\uD569\uB2C8\uB2E4"
    );
    updateDirtyState();
  }
  function updateAllVisuals() {
    if (!draft) return;
    document.querySelectorAll("[data-visual-value]").forEach((output) => {
      const visual = output.closest("[data-path]");
      const path = visual?.dataset.path;
      if (path) output.textContent = fields[path].format(getValue(draft, path));
    });
    document.querySelectorAll("[data-marker]").forEach((marker) => {
      const owner = marker.closest(".policy-visual");
      const path = marker.dataset.path ?? owner?.dataset.path;
      if (!path) return;
      const field = fields[path];
      const minimum = Number(owner?.dataset.min ?? field.min);
      const maximum = Number(owner?.dataset.max ?? field.max);
      marker.style.left = `${position(getValue(draft, path), minimum, maximum, owner?.dataset.log === "true")}%`;
    });
    document.querySelectorAll("[data-dual-value]").forEach((output) => {
      const visual = output.closest(".policy-visual");
      const paths = Array.from(visual?.querySelectorAll("[data-path]") ?? []).map(
        (item) => item.dataset.path
      );
      output.textContent = paths.map((path) => fields[path].format(getValue(draft, path))).join(" / ");
    });
    Object.keys(fields).forEach((path) => {
      const id = path.replaceAll(".", "-");
      const output = document.querySelector(`#${id}-readout`);
      if (output) output.value = fields[path].format(getValue(draft, path));
    });
    updateOverviewSummary();
  }
  function updateOverviewSummary() {
    if (!draft) return;
    const items = document.querySelectorAll(".summary-item strong");
    const values = [
      formatDuration(draft.collection.file_reconcile_interval_ms),
      `${formatNumber(draft.collection.max_batch_records)}\uAC1C`,
      formatBytes(draft.collection.local_storage_budget_bytes),
      `${formatNumber(draft.retention.max_record_age_days)}\uC77C`
    ];
    items.forEach((item, index) => {
      item.textContent = values[index] ?? "";
    });
  }
  function updateDirtyState() {
    if (!draft || !persisted) return;
    const changed = changedPaths(draft, persisted);
    const dirty = draft.enabled !== persisted.enabled || changed.length > 0;
    document.querySelector("#save-band")?.classList.toggle("dirty", dirty);
    setText("save-title", conflicted ? "\uC678\uBD80 \uBCC0\uACBD \uAC10\uC9C0" : dirty ? `${changed.length + Number(draft.enabled !== persisted.enabled)}\uAC1C \uBCC0\uACBD` : "\uC800\uC7A5\uB428");
    setText("save-detail", conflicted ? "\uCD5C\uC2E0 \uC124\uC815\uC744 \uB2E4\uC2DC \uBD88\uB7EC\uC628 \uB4A4 \uD3B8\uC9D1\uD558\uC138\uC694." : dirty ? "\uC800\uC7A5 \uC804\uAE4C\uC9C0 \uC774 \uBE0C\uB77C\uC6B0\uC800\uC5D0\uB9CC \uC720\uC9C0\uB429\uB2C8\uB2E4." : "\uD604\uC7AC \uC124\uC815\uACFC \uAC19\uC2B5\uB2C8\uB2E4.");
    setDisabled("save", !dirty || busy || conflicted);
    setDisabled("discard", !dirty || busy);
    setDisabled("reset", busy);
    document.querySelectorAll("[data-field]").forEach((row) => {
      row.classList.toggle("changed", changed.includes(row.dataset.field));
    });
  }
  async function saveDraft() {
    if (!draft || busy || conflicted) return;
    clearErrors();
    const form = document.querySelector("#settings-form");
    if (form && !form.checkValidity()) {
      form.reportValidity();
      showToast("\uBE44\uC5B4 \uC788\uAC70\uB098 \uD5C8\uC6A9 \uBC94\uC704\uB97C \uBC97\uC5B4\uB09C \uAC12\uC744 \uD655\uC778\uD558\uC138\uC694.", "error");
      return;
    }
    if (!validate_local_runtime_config_v2_default(draft)) {
      const errors = validate_local_runtime_config_v2_default.errors ?? [];
      for (const error of errors) {
        const path = error.instancePath?.replace(/^\//, "").replaceAll("/", ".");
        if (path in fields) showFieldError(path, error.message ?? "\uD5C8\uC6A9 \uBC94\uC704\uB97C \uD655\uC778\uD558\uC138\uC694.");
      }
      focusFirstInvalid();
      showToast("\uD5C8\uC6A9 \uBC94\uC704\uB97C \uBC97\uC5B4\uB09C \uAC12\uC744 \uD655\uC778\uD558\uC138\uC694.", "error");
      return;
    }
    busy = true;
    setBusy(true);
    try {
      const envelope = await api("/api/config", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ config: draft, revision })
      });
      applyEnvelope(envelope);
      renderSettings("save-title");
      showToast("\uC124\uC815\uC744 \uC800\uC7A5\uD588\uC2B5\uB2C8\uB2E4.", "success");
    } catch (error) {
      const apiError = error;
      if (apiError.code === "config_conflict") {
        await rebaseDraftOnLatest();
        showToast("\uCD5C\uC2E0 \uC124\uC815\uC744 \uBD88\uB7EC\uC640 \uB0B4 \uBCC0\uACBD\uB9CC \uB2E4\uC2DC \uC801\uC6A9\uD588\uC2B5\uB2C8\uB2E4. \uAC80\uD1A0 \uD6C4 \uC800\uC7A5\uD558\uC138\uC694.", "error");
      } else if (apiError.code === "invalid_session") {
        token = "";
        renderExpired();
        return;
      } else {
        showToast(messageOf(error), "error");
      }
    } finally {
      busy = false;
      setBusy(false);
      updateDirtyState();
    }
  }
  async function rebaseDraftOnLatest() {
    if (!draft || !persisted) return;
    const localDraft = structuredClone(draft);
    const localBase = structuredClone(persisted);
    const changed = changedPaths(localDraft, localBase);
    const enabledChanged = localDraft.enabled !== localBase.enabled;
    const latest = await api("/api/config");
    applyEnvelope(latest);
    if (!draft) return;
    for (const path of changed) setValue(draft, path, getValue(localDraft, path));
    if (enabledChanged) draft.enabled = localDraft.enabled;
    conflicted = false;
    renderSettings("save-title");
  }
  function discardChanges() {
    if (!persisted) return;
    draft = structuredClone(persisted);
    conflicted = false;
    renderSettings("save-title");
    showToast("\uC800\uC7A5\uD558\uC9C0 \uC54A\uC740 \uBCC0\uACBD\uC744 \uCDE8\uC18C\uD588\uC2B5\uB2C8\uB2E4.", "neutral");
  }
  function openResetDialog() {
    document.querySelector("#reset-dialog")?.showModal();
  }
  function closeResetDialog() {
    document.querySelector("#reset-dialog")?.close();
    document.querySelector("#reset")?.focus();
  }
  function resetDefaults() {
    if (!defaults) return;
    draft = structuredClone(defaults);
    closeResetDialog();
    renderSettings("reset");
    showToast("\uAE30\uBCF8\uAC12\uC744 \uD3B8\uC9D1\uAC12\uC5D0 \uC801\uC6A9\uD588\uC2B5\uB2C8\uB2E4. \uC800\uC7A5\uD574\uC57C \uBC18\uC601\uB429\uB2C8\uB2E4.", "neutral");
  }
  async function closeSession() {
    window.clearInterval(heartbeatTimer);
    try {
      await api("/api/shutdown", { method: "POST" });
    } catch {
    }
    token = "";
    renderExpired();
  }
  function requestCloseSession() {
    if (isDirty()) {
      document.querySelector("#close-dialog")?.showModal();
    } else {
      void closeSession();
    }
  }
  function closeCloseDialog() {
    document.querySelector("#close-dialog")?.close();
    document.querySelector("#close-session")?.focus();
  }
  async function heartbeat() {
    if (Date.now() - lastUserActivity >= 6e4) return;
    try {
      await api("/api/heartbeat", { method: "POST" });
    } catch {
      window.clearInterval(heartbeatTimer);
      token = "";
      renderExpired();
    }
  }
  function setActiveNavigation(hash) {
    document.querySelectorAll(".section-nav a").forEach((item) => {
      const active = item.hash === hash;
      item.classList.toggle("active", active);
      if (active) item.setAttribute("aria-current", "page");
      else item.removeAttribute("aria-current");
    });
  }
  function isDirty() {
    return Boolean(
      draft && persisted && (draft.enabled !== persisted.enabled || changedPaths(draft, persisted).length > 0)
    );
  }
  async function api(path, init = {}) {
    const headers = new Headers(init.headers);
    headers.set("x-agent-observability-session", token);
    const response = await fetch(path, { ...init, headers, cache: "no-store" });
    if (!response.ok) {
      const body = await response.json().catch(() => ({}));
      const error = new Error(body.message ?? `\uC694\uCCAD\uC774 \uC2E4\uD328\uD588\uC2B5\uB2C8\uB2E4 (${response.status}).`);
      if (body.code) error.code = body.code;
      throw error;
    }
    if (response.status === 204) return void 0;
    return await response.json();
  }
  function applyEnvelope(envelope) {
    persisted = structuredClone(envelope.config);
    draft = structuredClone(envelope.config);
    defaults = structuredClone(envelope.defaults);
    revision = envelope.revision;
    conflicted = false;
  }
  function setBusy(value) {
    document.querySelector("#settings-form")?.setAttribute("aria-busy", String(value));
    setText("save-title", value ? "\uC800\uC7A5 \uC911" : "\uC800\uC7A5\uB428");
    document.querySelectorAll("button").forEach((button) => {
      if (button.id !== "close-session") button.disabled = value;
    });
  }
  function showFieldError(path, message) {
    const id = path.replaceAll(".", "-");
    const input = document.querySelector(`#${id}`);
    input?.setAttribute("aria-invalid", "true");
    input?.setAttribute("aria-describedby", `${id}-help ${id}-readout ${id}-error`);
    setText(`${id}-error`, message);
  }
  function clearFieldError(path) {
    const id = path.replaceAll(".", "-");
    document.querySelector(`#${id}`)?.removeAttribute("aria-invalid");
    setText(`${id}-error`, "");
  }
  function clearErrors() {
    Object.keys(fields).forEach(clearFieldError);
  }
  function focusFirstInvalid() {
    document.querySelector("[aria-invalid=true]")?.focus();
  }
  function showToast(message, kind) {
    const toast = document.querySelector("#toast");
    if (!toast) return;
    toast.textContent = message;
    toast.dataset.kind = kind;
    toast.classList.add("visible");
    window.setTimeout(() => toast.classList.remove("visible"), 4e3);
  }
  function mountIcons() {
    createIcons({
      icons: {
        Activity,
        Archive,
        Check,
        Database,
        Gauge,
        HeartPulse,
        RefreshCw,
        RotateCcw,
        Save,
        Settings2,
        ShieldCheck,
        SlidersHorizontal,
        X,
        XCircle: CircleX
      },
      attrs: { "stroke-width": 1.8 }
    });
  }
  function getValue(config, path) {
    const [group, key] = path.split(".");
    return Number(config[group][key]);
  }
  function setValue(config, path, value) {
    const [group, key] = path.split(".");
    config[group][key] = value;
  }
  function changedPaths(left, right) {
    return Object.keys(fields).filter(
      (path) => getValue(left, path) !== getValue(right, path)
    );
  }
  function position(value, min, max, logarithmic) {
    const bounded = Math.min(max, Math.max(min, value));
    const ratio = logarithmic ? (Math.log(bounded) - Math.log(min)) / (Math.log(max) - Math.log(min)) : (bounded - min) / (max - min);
    return 4 + ratio * 92;
  }
  function formatDuration(value) {
    if (value >= 6e4 && value % 6e4 === 0) return `${formatNumber(value / 6e4)}\uBD84`;
    if (value >= 1e3) return `${formatNumber(value / 1e3)}\uCD08`;
    return `${formatNumber(value)}ms`;
  }
  function formatBytes(value) {
    if (value >= 1073741824) return `${formatDecimal(value / 1073741824)} GiB`;
    if (value >= 1048576) return `${formatDecimal(value / 1048576)} MiB`;
    return `${formatDecimal(value / 1024)} KiB`;
  }
  function formatNumber(value) {
    return new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 0 }).format(value);
  }
  function formatDecimal(value) {
    return new Intl.NumberFormat("ko-KR", { maximumFractionDigits: 2 }).format(value);
  }
  function setText(id, value) {
    const element = document.querySelector(`#${id}`);
    if (element) element.textContent = value;
  }
  function setDisabled(id, value) {
    const button = document.querySelector(`#${id}`);
    if (button) button.disabled = value;
  }
  function messageOf(error) {
    return error instanceof Error ? error.message : "\uC54C \uC218 \uC5C6\uB294 \uC624\uB958\uAC00 \uBC1C\uC0DD\uD588\uC2B5\uB2C8\uB2E4.";
  }
})();
