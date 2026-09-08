/* Generated from contracts/local-runtime-config-v5.schema.json. Do not edit. */
"use strict";
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

  // node_modules/lucide/dist/esm/icons/cable.mjs
  var Cable = [
    ["path", { d: "M17 19a1 1 0 0 1-1-1v-2a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2a1 1 0 0 1-1 1z" }],
    ["path", { d: "M17 21v-2" }],
    ["path", { d: "M19 14V6.5a1 1 0 0 0-7 0v11a1 1 0 0 1-7 0V10" }],
    ["path", { d: "M21 21v-2" }],
    ["path", { d: "M3 5V3" }],
    ["path", { d: "M4 10a2 2 0 0 1-2-2V6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2a2 2 0 0 1-2 2z" }],
    ["path", { d: "M7 5V3" }]
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

  // node_modules/lucide/dist/esm/icons/external-link.mjs
  var ExternalLink = [
    ["path", { d: "M15 3h6v6" }],
    ["path", { d: "M10 14 21 3" }],
    ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }]
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

  // node_modules/lucide/dist/esm/icons/monitor-up.mjs
  var MonitorUp = [
    ["path", { d: "m9 10 3-3 3 3" }],
    ["path", { d: "M12 13V7" }],
    ["rect", { width: "20", height: "14", x: "2", y: "3", rx: "2" }],
    ["path", { d: "M12 17v4" }],
    ["path", { d: "M8 21h8" }]
  ];

  // node_modules/lucide/dist/esm/icons/power.mjs
  var Power = [
    ["path", { d: "M12 2v10" }],
    ["path", { d: "M18.4 6.6a9 9 0 1 1-12.77.04" }]
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

  // ui/settings/generated/validate-local-runtime-config-v5.js
  var validate_local_runtime_config_v5_default = validate20;
  var schema33 = { "type": "object", "additionalProperties": false, "required": ["mode", "retained_target_bytes", "workspace_budget_bytes", "minimum_free_bytes"], "properties": { "mode": { "type": "string", "enum": ["legacy", "separated"] }, "retained_target_bytes": { "type": "integer", "minimum": 268435456, "maximum": 21474836480 }, "workspace_budget_bytes": { "type": "integer", "minimum": 268435456, "maximum": 21474836480 }, "minimum_free_bytes": { "type": "integer", "minimum": 268435456, "maximum": 21474836480 } } };
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
        if (data.schema_version === void 0 && (missing0 = "schema_version") || data.enabled === void 0 && (missing0 = "enabled") || data.capture_private_codex_turn_details === void 0 && (missing0 = "capture_private_codex_turn_details") || data.collection === void 0 && (missing0 = "collection") || data.storage_budget === void 0 && (missing0 = "storage_budget") || data.retention === void 0 && (missing0 = "retention") || data.lifecycle === void 0 && (missing0 = "lifecycle")) {
          validate20.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          const _errs1 = errors;
          for (const key0 in data) {
            if (!(key0 === "schema_version" || key0 === "enabled" || key0 === "capture_private_codex_turn_details" || key0 === "collection" || key0 === "storage_budget" || key0 === "retention" || key0 === "lifecycle")) {
              validate20.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
              return false;
              break;
            }
          }
          if (_errs1 === errors) {
            if (data.schema_version !== void 0) {
              const _errs2 = errors;
              if ("local_runtime.v5" !== data.schema_version) {
                validate20.errors = [{ instancePath: instancePath + "/schema_version", schemaPath: "#/properties/schema_version/const", keyword: "const", params: { allowedValue: "local_runtime.v5" }, message: "must be equal to constant" }];
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
                if (data.capture_private_codex_turn_details !== void 0) {
                  const _errs5 = errors;
                  if (typeof data.capture_private_codex_turn_details !== "boolean") {
                    validate20.errors = [{ instancePath: instancePath + "/capture_private_codex_turn_details", schemaPath: "#/properties/capture_private_codex_turn_details/type", keyword: "type", params: { type: "boolean" }, message: "must be boolean" }];
                    return false;
                  }
                  var valid0 = _errs5 === errors;
                } else {
                  var valid0 = true;
                }
                if (valid0) {
                  if (data.collection !== void 0) {
                    let data3 = data.collection;
                    const _errs7 = errors;
                    const _errs8 = errors;
                    if (errors === _errs8) {
                      if (data3 && typeof data3 == "object" && !Array.isArray(data3)) {
                        let missing1;
                        if (data3.file_reconcile_interval_ms === void 0 && (missing1 = "file_reconcile_interval_ms") || data3.flush_interval_ms === void 0 && (missing1 = "flush_interval_ms") || data3.max_batch_records === void 0 && (missing1 = "max_batch_records") || data3.max_batch_bytes === void 0 && (missing1 = "max_batch_bytes") || data3.active_heartbeat_interval_ms === void 0 && (missing1 = "active_heartbeat_interval_ms") || data3.idle_heartbeat_interval_ms === void 0 && (missing1 = "idle_heartbeat_interval_ms") || data3.local_storage_budget_bytes === void 0 && (missing1 = "local_storage_budget_bytes")) {
                          validate20.errors = [{ instancePath: instancePath + "/collection", schemaPath: "#/$defs/collection/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
                          return false;
                        } else {
                          const _errs10 = errors;
                          for (const key1 in data3) {
                            if (!(key1 === "file_reconcile_interval_ms" || key1 === "flush_interval_ms" || key1 === "max_batch_records" || key1 === "max_batch_bytes" || key1 === "active_heartbeat_interval_ms" || key1 === "idle_heartbeat_interval_ms" || key1 === "local_storage_budget_bytes")) {
                              validate20.errors = [{ instancePath: instancePath + "/collection", schemaPath: "#/$defs/collection/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key1 }, message: "must NOT have additional properties" }];
                              return false;
                              break;
                            }
                          }
                          if (_errs10 === errors) {
                            if (data3.file_reconcile_interval_ms !== void 0) {
                              let data4 = data3.file_reconcile_interval_ms;
                              const _errs11 = errors;
                              if (!(typeof data4 == "number" && (!(data4 % 1) && !isNaN(data4)) && isFinite(data4))) {
                                validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                return false;
                              }
                              if (errors === _errs11) {
                                if (typeof data4 == "number" && isFinite(data4)) {
                                  if (data4 > 6e4 || isNaN(data4)) {
                                    validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 6e4 }, message: "must be <= 60000" }];
                                    return false;
                                  } else {
                                    if (data4 < 1e3 || isNaN(data4)) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/file_reconcile_interval_ms", schemaPath: "#/$defs/collection/properties/file_reconcile_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1e3 }, message: "must be >= 1000" }];
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
                              if (data3.flush_interval_ms !== void 0) {
                                let data5 = data3.flush_interval_ms;
                                const _errs13 = errors;
                                if (!(typeof data5 == "number" && (!(data5 % 1) && !isNaN(data5)) && isFinite(data5))) {
                                  validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                  return false;
                                }
                                if (errors === _errs13) {
                                  if (typeof data5 == "number" && isFinite(data5)) {
                                    if (data5 > 6e4 || isNaN(data5)) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 6e4 }, message: "must be <= 60000" }];
                                      return false;
                                    } else {
                                      if (data5 < 1e3 || isNaN(data5)) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/flush_interval_ms", schemaPath: "#/$defs/collection/properties/flush_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1e3 }, message: "must be >= 1000" }];
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
                                if (data3.max_batch_records !== void 0) {
                                  let data6 = data3.max_batch_records;
                                  const _errs15 = errors;
                                  if (!(typeof data6 == "number" && (!(data6 % 1) && !isNaN(data6)) && isFinite(data6))) {
                                    validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs15) {
                                    if (typeof data6 == "number" && isFinite(data6)) {
                                      if (data6 > 500 || isNaN(data6)) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/maximum", keyword: "maximum", params: { comparison: "<=", limit: 500 }, message: "must be <= 500" }];
                                        return false;
                                      } else {
                                        if (data6 < 1 || isNaN(data6)) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_records", schemaPath: "#/$defs/collection/properties/max_batch_records/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
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
                                  if (data3.max_batch_bytes !== void 0) {
                                    let data7 = data3.max_batch_bytes;
                                    const _errs17 = errors;
                                    if (!(typeof data7 == "number" && (!(data7 % 1) && !isNaN(data7)) && isFinite(data7))) {
                                      validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                      return false;
                                    }
                                    if (errors === _errs17) {
                                      if (typeof data7 == "number" && isFinite(data7)) {
                                        if (data7 > 2097152 || isNaN(data7)) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 2097152 }, message: "must be <= 2097152" }];
                                          return false;
                                        } else {
                                          if (data7 < 16384 || isNaN(data7)) {
                                            validate20.errors = [{ instancePath: instancePath + "/collection/max_batch_bytes", schemaPath: "#/$defs/collection/properties/max_batch_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 16384 }, message: "must be >= 16384" }];
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
                                    if (data3.active_heartbeat_interval_ms !== void 0) {
                                      let data8 = data3.active_heartbeat_interval_ms;
                                      const _errs19 = errors;
                                      if (!(typeof data8 == "number" && (!(data8 % 1) && !isNaN(data8)) && isFinite(data8))) {
                                        validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                        return false;
                                      }
                                      if (errors === _errs19) {
                                        if (typeof data8 == "number" && isFinite(data8)) {
                                          if (data8 > 3e5 || isNaN(data8)) {
                                            validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3e5 }, message: "must be <= 300000" }];
                                            return false;
                                          } else {
                                            if (data8 < 3e4 || isNaN(data8)) {
                                              validate20.errors = [{ instancePath: instancePath + "/collection/active_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/active_heartbeat_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 3e4 }, message: "must be >= 30000" }];
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
                                      if (data3.idle_heartbeat_interval_ms !== void 0) {
                                        let data9 = data3.idle_heartbeat_interval_ms;
                                        const _errs21 = errors;
                                        if (!(typeof data9 == "number" && (!(data9 % 1) && !isNaN(data9)) && isFinite(data9))) {
                                          validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                          return false;
                                        }
                                        if (errors === _errs21) {
                                          if (typeof data9 == "number" && isFinite(data9)) {
                                            if (data9 > 9e5 || isNaN(data9)) {
                                              validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/maximum", keyword: "maximum", params: { comparison: "<=", limit: 9e5 }, message: "must be <= 900000" }];
                                              return false;
                                            } else {
                                              if (data9 < 12e4 || isNaN(data9)) {
                                                validate20.errors = [{ instancePath: instancePath + "/collection/idle_heartbeat_interval_ms", schemaPath: "#/$defs/collection/properties/idle_heartbeat_interval_ms/minimum", keyword: "minimum", params: { comparison: ">=", limit: 12e4 }, message: "must be >= 120000" }];
                                                return false;
                                              }
                                            }
                                          }
                                        }
                                        var valid2 = _errs21 === errors;
                                      } else {
                                        var valid2 = true;
                                      }
                                      if (valid2) {
                                        if (data3.local_storage_budget_bytes !== void 0) {
                                          let data10 = data3.local_storage_budget_bytes;
                                          const _errs23 = errors;
                                          if (!(typeof data10 == "number" && (!(data10 % 1) && !isNaN(data10)) && isFinite(data10))) {
                                            validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                            return false;
                                          }
                                          if (errors === _errs23) {
                                            if (typeof data10 == "number" && isFinite(data10)) {
                                              if (data10 > 21474836480 || isNaN(data10)) {
                                                validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 21474836480 }, message: "must be <= 21474836480" }];
                                                return false;
                                              } else {
                                                if (data10 < 268435456 || isNaN(data10)) {
                                                  validate20.errors = [{ instancePath: instancePath + "/collection/local_storage_budget_bytes", schemaPath: "#/$defs/collection/properties/local_storage_budget_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 268435456 }, message: "must be >= 268435456" }];
                                                  return false;
                                                }
                                              }
                                            }
                                          }
                                          var valid2 = _errs23 === errors;
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
                    var valid0 = _errs7 === errors;
                  } else {
                    var valid0 = true;
                  }
                  if (valid0) {
                    if (data.storage_budget !== void 0) {
                      let data11 = data.storage_budget;
                      const _errs25 = errors;
                      const _errs26 = errors;
                      if (errors === _errs26) {
                        if (data11 && typeof data11 == "object" && !Array.isArray(data11)) {
                          let missing2;
                          if (data11.mode === void 0 && (missing2 = "mode") || data11.retained_target_bytes === void 0 && (missing2 = "retained_target_bytes") || data11.workspace_budget_bytes === void 0 && (missing2 = "workspace_budget_bytes") || data11.minimum_free_bytes === void 0 && (missing2 = "minimum_free_bytes")) {
                            validate20.errors = [{ instancePath: instancePath + "/storage_budget", schemaPath: "#/$defs/storage_budget/required", keyword: "required", params: { missingProperty: missing2 }, message: "must have required property '" + missing2 + "'" }];
                            return false;
                          } else {
                            const _errs28 = errors;
                            for (const key2 in data11) {
                              if (!(key2 === "mode" || key2 === "retained_target_bytes" || key2 === "workspace_budget_bytes" || key2 === "minimum_free_bytes")) {
                                validate20.errors = [{ instancePath: instancePath + "/storage_budget", schemaPath: "#/$defs/storage_budget/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key2 }, message: "must NOT have additional properties" }];
                                return false;
                                break;
                              }
                            }
                            if (_errs28 === errors) {
                              if (data11.mode !== void 0) {
                                let data12 = data11.mode;
                                const _errs29 = errors;
                                if (typeof data12 !== "string") {
                                  validate20.errors = [{ instancePath: instancePath + "/storage_budget/mode", schemaPath: "#/$defs/storage_budget/properties/mode/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                  return false;
                                }
                                if (!(data12 === "legacy" || data12 === "separated")) {
                                  validate20.errors = [{ instancePath: instancePath + "/storage_budget/mode", schemaPath: "#/$defs/storage_budget/properties/mode/enum", keyword: "enum", params: { allowedValues: schema33.properties.mode.enum }, message: "must be equal to one of the allowed values" }];
                                  return false;
                                }
                                var valid4 = _errs29 === errors;
                              } else {
                                var valid4 = true;
                              }
                              if (valid4) {
                                if (data11.retained_target_bytes !== void 0) {
                                  let data13 = data11.retained_target_bytes;
                                  const _errs31 = errors;
                                  if (!(typeof data13 == "number" && (!(data13 % 1) && !isNaN(data13)) && isFinite(data13))) {
                                    validate20.errors = [{ instancePath: instancePath + "/storage_budget/retained_target_bytes", schemaPath: "#/$defs/storage_budget/properties/retained_target_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs31) {
                                    if (typeof data13 == "number" && isFinite(data13)) {
                                      if (data13 > 21474836480 || isNaN(data13)) {
                                        validate20.errors = [{ instancePath: instancePath + "/storage_budget/retained_target_bytes", schemaPath: "#/$defs/storage_budget/properties/retained_target_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 21474836480 }, message: "must be <= 21474836480" }];
                                        return false;
                                      } else {
                                        if (data13 < 268435456 || isNaN(data13)) {
                                          validate20.errors = [{ instancePath: instancePath + "/storage_budget/retained_target_bytes", schemaPath: "#/$defs/storage_budget/properties/retained_target_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 268435456 }, message: "must be >= 268435456" }];
                                          return false;
                                        }
                                      }
                                    }
                                  }
                                  var valid4 = _errs31 === errors;
                                } else {
                                  var valid4 = true;
                                }
                                if (valid4) {
                                  if (data11.workspace_budget_bytes !== void 0) {
                                    let data14 = data11.workspace_budget_bytes;
                                    const _errs33 = errors;
                                    if (!(typeof data14 == "number" && (!(data14 % 1) && !isNaN(data14)) && isFinite(data14))) {
                                      validate20.errors = [{ instancePath: instancePath + "/storage_budget/workspace_budget_bytes", schemaPath: "#/$defs/storage_budget/properties/workspace_budget_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                      return false;
                                    }
                                    if (errors === _errs33) {
                                      if (typeof data14 == "number" && isFinite(data14)) {
                                        if (data14 > 21474836480 || isNaN(data14)) {
                                          validate20.errors = [{ instancePath: instancePath + "/storage_budget/workspace_budget_bytes", schemaPath: "#/$defs/storage_budget/properties/workspace_budget_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 21474836480 }, message: "must be <= 21474836480" }];
                                          return false;
                                        } else {
                                          if (data14 < 268435456 || isNaN(data14)) {
                                            validate20.errors = [{ instancePath: instancePath + "/storage_budget/workspace_budget_bytes", schemaPath: "#/$defs/storage_budget/properties/workspace_budget_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 268435456 }, message: "must be >= 268435456" }];
                                            return false;
                                          }
                                        }
                                      }
                                    }
                                    var valid4 = _errs33 === errors;
                                  } else {
                                    var valid4 = true;
                                  }
                                  if (valid4) {
                                    if (data11.minimum_free_bytes !== void 0) {
                                      let data15 = data11.minimum_free_bytes;
                                      const _errs35 = errors;
                                      if (!(typeof data15 == "number" && (!(data15 % 1) && !isNaN(data15)) && isFinite(data15))) {
                                        validate20.errors = [{ instancePath: instancePath + "/storage_budget/minimum_free_bytes", schemaPath: "#/$defs/storage_budget/properties/minimum_free_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                        return false;
                                      }
                                      if (errors === _errs35) {
                                        if (typeof data15 == "number" && isFinite(data15)) {
                                          if (data15 > 21474836480 || isNaN(data15)) {
                                            validate20.errors = [{ instancePath: instancePath + "/storage_budget/minimum_free_bytes", schemaPath: "#/$defs/storage_budget/properties/minimum_free_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 21474836480 }, message: "must be <= 21474836480" }];
                                            return false;
                                          } else {
                                            if (data15 < 268435456 || isNaN(data15)) {
                                              validate20.errors = [{ instancePath: instancePath + "/storage_budget/minimum_free_bytes", schemaPath: "#/$defs/storage_budget/properties/minimum_free_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 268435456 }, message: "must be >= 268435456" }];
                                              return false;
                                            }
                                          }
                                        }
                                      }
                                      var valid4 = _errs35 === errors;
                                    } else {
                                      var valid4 = true;
                                    }
                                  }
                                }
                              }
                            }
                          }
                        } else {
                          validate20.errors = [{ instancePath: instancePath + "/storage_budget", schemaPath: "#/$defs/storage_budget/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                          return false;
                        }
                      }
                      var valid0 = _errs25 === errors;
                    } else {
                      var valid0 = true;
                    }
                    if (valid0) {
                      if (data.retention !== void 0) {
                        let data16 = data.retention;
                        const _errs37 = errors;
                        const _errs38 = errors;
                        if (errors === _errs38) {
                          if (data16 && typeof data16 == "object" && !Array.isArray(data16)) {
                            let missing3;
                            if (data16.max_record_age_days === void 0 && (missing3 = "max_record_age_days") || data16.max_archive_records === void 0 && (missing3 = "max_archive_records") || data16.max_archive_bytes === void 0 && (missing3 = "max_archive_bytes")) {
                              validate20.errors = [{ instancePath: instancePath + "/retention", schemaPath: "#/$defs/retention/required", keyword: "required", params: { missingProperty: missing3 }, message: "must have required property '" + missing3 + "'" }];
                              return false;
                            } else {
                              const _errs40 = errors;
                              for (const key3 in data16) {
                                if (!(key3 === "max_record_age_days" || key3 === "max_archive_records" || key3 === "max_archive_bytes")) {
                                  validate20.errors = [{ instancePath: instancePath + "/retention", schemaPath: "#/$defs/retention/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key3 }, message: "must NOT have additional properties" }];
                                  return false;
                                  break;
                                }
                              }
                              if (_errs40 === errors) {
                                if (data16.max_record_age_days !== void 0) {
                                  let data17 = data16.max_record_age_days;
                                  const _errs41 = errors;
                                  if (!(typeof data17 == "number" && (!(data17 % 1) && !isNaN(data17)) && isFinite(data17))) {
                                    validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                    return false;
                                  }
                                  if (errors === _errs41) {
                                    if (typeof data17 == "number" && isFinite(data17)) {
                                      if (data17 > 3650 || isNaN(data17)) {
                                        validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                        return false;
                                      } else {
                                        if (data17 < 1 || isNaN(data17)) {
                                          validate20.errors = [{ instancePath: instancePath + "/retention/max_record_age_days", schemaPath: "#/$defs/retention/properties/max_record_age_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                          return false;
                                        }
                                      }
                                    }
                                  }
                                  var valid6 = _errs41 === errors;
                                } else {
                                  var valid6 = true;
                                }
                                if (valid6) {
                                  if (data16.max_archive_records !== void 0) {
                                    let data18 = data16.max_archive_records;
                                    const _errs43 = errors;
                                    if (!(typeof data18 == "number" && (!(data18 % 1) && !isNaN(data18)) && isFinite(data18))) {
                                      validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                      return false;
                                    }
                                    if (errors === _errs43) {
                                      if (typeof data18 == "number" && isFinite(data18)) {
                                        if (data18 > 1e5 || isNaN(data18)) {
                                          validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/maximum", keyword: "maximum", params: { comparison: "<=", limit: 1e5 }, message: "must be <= 100000" }];
                                          return false;
                                        } else {
                                          if (data18 < 1 || isNaN(data18)) {
                                            validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_records", schemaPath: "#/$defs/retention/properties/max_archive_records/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                            return false;
                                          }
                                        }
                                      }
                                    }
                                    var valid6 = _errs43 === errors;
                                  } else {
                                    var valid6 = true;
                                  }
                                  if (valid6) {
                                    if (data16.max_archive_bytes !== void 0) {
                                      let data19 = data16.max_archive_bytes;
                                      const _errs45 = errors;
                                      if (!(typeof data19 == "number" && (!(data19 % 1) && !isNaN(data19)) && isFinite(data19))) {
                                        validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                        return false;
                                      }
                                      if (errors === _errs45) {
                                        if (typeof data19 == "number" && isFinite(data19)) {
                                          if (data19 > 268435456 || isNaN(data19)) {
                                            validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/maximum", keyword: "maximum", params: { comparison: "<=", limit: 268435456 }, message: "must be <= 268435456" }];
                                            return false;
                                          } else {
                                            if (data19 < 65536 || isNaN(data19)) {
                                              validate20.errors = [{ instancePath: instancePath + "/retention/max_archive_bytes", schemaPath: "#/$defs/retention/properties/max_archive_bytes/minimum", keyword: "minimum", params: { comparison: ">=", limit: 65536 }, message: "must be >= 65536" }];
                                              return false;
                                            }
                                          }
                                        }
                                      }
                                      var valid6 = _errs45 === errors;
                                    } else {
                                      var valid6 = true;
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
                        var valid0 = _errs37 === errors;
                      } else {
                        var valid0 = true;
                      }
                      if (valid0) {
                        if (data.lifecycle !== void 0) {
                          let data20 = data.lifecycle;
                          const _errs47 = errors;
                          const _errs48 = errors;
                          if (errors === _errs48) {
                            if (data20 && typeof data20 == "object" && !Array.isArray(data20)) {
                              let missing4;
                              if (data20.enabled === void 0 && (missing4 = "enabled") || data20.hot_days === void 0 && (missing4 = "hot_days") || data20.warm_days === void 0 && (missing4 = "warm_days") || data20.delete_after_days === void 0 && (missing4 = "delete_after_days") || data20.private_raw_days === void 0 && (missing4 = "private_raw_days") || data20.maintenance_interval_seconds === void 0 && (missing4 = "maintenance_interval_seconds") || data20.max_traces_per_pass === void 0 && (missing4 = "max_traces_per_pass")) {
                                validate20.errors = [{ instancePath: instancePath + "/lifecycle", schemaPath: "#/$defs/lifecycle/required", keyword: "required", params: { missingProperty: missing4 }, message: "must have required property '" + missing4 + "'" }];
                                return false;
                              } else {
                                const _errs50 = errors;
                                for (const key4 in data20) {
                                  if (!(key4 === "enabled" || key4 === "hot_days" || key4 === "warm_days" || key4 === "delete_after_days" || key4 === "private_raw_days" || key4 === "maintenance_interval_seconds" || key4 === "max_traces_per_pass")) {
                                    validate20.errors = [{ instancePath: instancePath + "/lifecycle", schemaPath: "#/$defs/lifecycle/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key4 }, message: "must NOT have additional properties" }];
                                    return false;
                                    break;
                                  }
                                }
                                if (_errs50 === errors) {
                                  if (data20.enabled !== void 0) {
                                    const _errs51 = errors;
                                    if (typeof data20.enabled !== "boolean") {
                                      validate20.errors = [{ instancePath: instancePath + "/lifecycle/enabled", schemaPath: "#/$defs/lifecycle/properties/enabled/type", keyword: "type", params: { type: "boolean" }, message: "must be boolean" }];
                                      return false;
                                    }
                                    var valid8 = _errs51 === errors;
                                  } else {
                                    var valid8 = true;
                                  }
                                  if (valid8) {
                                    if (data20.hot_days !== void 0) {
                                      let data22 = data20.hot_days;
                                      const _errs53 = errors;
                                      if (!(typeof data22 == "number" && (!(data22 % 1) && !isNaN(data22)) && isFinite(data22))) {
                                        validate20.errors = [{ instancePath: instancePath + "/lifecycle/hot_days", schemaPath: "#/$defs/lifecycle/properties/hot_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                        return false;
                                      }
                                      if (errors === _errs53) {
                                        if (typeof data22 == "number" && isFinite(data22)) {
                                          if (data22 > 3650 || isNaN(data22)) {
                                            validate20.errors = [{ instancePath: instancePath + "/lifecycle/hot_days", schemaPath: "#/$defs/lifecycle/properties/hot_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                            return false;
                                          } else {
                                            if (data22 < 1 || isNaN(data22)) {
                                              validate20.errors = [{ instancePath: instancePath + "/lifecycle/hot_days", schemaPath: "#/$defs/lifecycle/properties/hot_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                              return false;
                                            }
                                          }
                                        }
                                      }
                                      var valid8 = _errs53 === errors;
                                    } else {
                                      var valid8 = true;
                                    }
                                    if (valid8) {
                                      if (data20.warm_days !== void 0) {
                                        let data23 = data20.warm_days;
                                        const _errs55 = errors;
                                        if (!(typeof data23 == "number" && (!(data23 % 1) && !isNaN(data23)) && isFinite(data23))) {
                                          validate20.errors = [{ instancePath: instancePath + "/lifecycle/warm_days", schemaPath: "#/$defs/lifecycle/properties/warm_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                          return false;
                                        }
                                        if (errors === _errs55) {
                                          if (typeof data23 == "number" && isFinite(data23)) {
                                            if (data23 > 3650 || isNaN(data23)) {
                                              validate20.errors = [{ instancePath: instancePath + "/lifecycle/warm_days", schemaPath: "#/$defs/lifecycle/properties/warm_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                              return false;
                                            } else {
                                              if (data23 < 1 || isNaN(data23)) {
                                                validate20.errors = [{ instancePath: instancePath + "/lifecycle/warm_days", schemaPath: "#/$defs/lifecycle/properties/warm_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                                return false;
                                              }
                                            }
                                          }
                                        }
                                        var valid8 = _errs55 === errors;
                                      } else {
                                        var valid8 = true;
                                      }
                                      if (valid8) {
                                        if (data20.delete_after_days !== void 0) {
                                          let data24 = data20.delete_after_days;
                                          const _errs57 = errors;
                                          if (!(typeof data24 == "number" && (!(data24 % 1) && !isNaN(data24)) && isFinite(data24))) {
                                            validate20.errors = [{ instancePath: instancePath + "/lifecycle/delete_after_days", schemaPath: "#/$defs/lifecycle/properties/delete_after_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                            return false;
                                          }
                                          if (errors === _errs57) {
                                            if (typeof data24 == "number" && isFinite(data24)) {
                                              if (data24 > 3650 || isNaN(data24)) {
                                                validate20.errors = [{ instancePath: instancePath + "/lifecycle/delete_after_days", schemaPath: "#/$defs/lifecycle/properties/delete_after_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                                return false;
                                              } else {
                                                if (data24 < 1 || isNaN(data24)) {
                                                  validate20.errors = [{ instancePath: instancePath + "/lifecycle/delete_after_days", schemaPath: "#/$defs/lifecycle/properties/delete_after_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                                  return false;
                                                }
                                              }
                                            }
                                          }
                                          var valid8 = _errs57 === errors;
                                        } else {
                                          var valid8 = true;
                                        }
                                        if (valid8) {
                                          if (data20.private_raw_days !== void 0) {
                                            let data25 = data20.private_raw_days;
                                            const _errs59 = errors;
                                            if (!(typeof data25 == "number" && (!(data25 % 1) && !isNaN(data25)) && isFinite(data25))) {
                                              validate20.errors = [{ instancePath: instancePath + "/lifecycle/private_raw_days", schemaPath: "#/$defs/lifecycle/properties/private_raw_days/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                              return false;
                                            }
                                            if (errors === _errs59) {
                                              if (typeof data25 == "number" && isFinite(data25)) {
                                                if (data25 > 3650 || isNaN(data25)) {
                                                  validate20.errors = [{ instancePath: instancePath + "/lifecycle/private_raw_days", schemaPath: "#/$defs/lifecycle/properties/private_raw_days/maximum", keyword: "maximum", params: { comparison: "<=", limit: 3650 }, message: "must be <= 3650" }];
                                                  return false;
                                                } else {
                                                  if (data25 < 1 || isNaN(data25)) {
                                                    validate20.errors = [{ instancePath: instancePath + "/lifecycle/private_raw_days", schemaPath: "#/$defs/lifecycle/properties/private_raw_days/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                                    return false;
                                                  }
                                                }
                                              }
                                            }
                                            var valid8 = _errs59 === errors;
                                          } else {
                                            var valid8 = true;
                                          }
                                          if (valid8) {
                                            if (data20.maintenance_interval_seconds !== void 0) {
                                              let data26 = data20.maintenance_interval_seconds;
                                              const _errs61 = errors;
                                              if (!(typeof data26 == "number" && (!(data26 % 1) && !isNaN(data26)) && isFinite(data26))) {
                                                validate20.errors = [{ instancePath: instancePath + "/lifecycle/maintenance_interval_seconds", schemaPath: "#/$defs/lifecycle/properties/maintenance_interval_seconds/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                                return false;
                                              }
                                              if (errors === _errs61) {
                                                if (typeof data26 == "number" && isFinite(data26)) {
                                                  if (data26 > 86400 || isNaN(data26)) {
                                                    validate20.errors = [{ instancePath: instancePath + "/lifecycle/maintenance_interval_seconds", schemaPath: "#/$defs/lifecycle/properties/maintenance_interval_seconds/maximum", keyword: "maximum", params: { comparison: "<=", limit: 86400 }, message: "must be <= 86400" }];
                                                    return false;
                                                  } else {
                                                    if (data26 < 60 || isNaN(data26)) {
                                                      validate20.errors = [{ instancePath: instancePath + "/lifecycle/maintenance_interval_seconds", schemaPath: "#/$defs/lifecycle/properties/maintenance_interval_seconds/minimum", keyword: "minimum", params: { comparison: ">=", limit: 60 }, message: "must be >= 60" }];
                                                      return false;
                                                    }
                                                  }
                                                }
                                              }
                                              var valid8 = _errs61 === errors;
                                            } else {
                                              var valid8 = true;
                                            }
                                            if (valid8) {
                                              if (data20.max_traces_per_pass !== void 0) {
                                                let data27 = data20.max_traces_per_pass;
                                                const _errs63 = errors;
                                                if (!(typeof data27 == "number" && (!(data27 % 1) && !isNaN(data27)) && isFinite(data27))) {
                                                  validate20.errors = [{ instancePath: instancePath + "/lifecycle/max_traces_per_pass", schemaPath: "#/$defs/lifecycle/properties/max_traces_per_pass/type", keyword: "type", params: { type: "integer" }, message: "must be integer" }];
                                                  return false;
                                                }
                                                if (errors === _errs63) {
                                                  if (typeof data27 == "number" && isFinite(data27)) {
                                                    if (data27 > 128 || isNaN(data27)) {
                                                      validate20.errors = [{ instancePath: instancePath + "/lifecycle/max_traces_per_pass", schemaPath: "#/$defs/lifecycle/properties/max_traces_per_pass/maximum", keyword: "maximum", params: { comparison: "<=", limit: 128 }, message: "must be <= 128" }];
                                                      return false;
                                                    } else {
                                                      if (data27 < 1 || isNaN(data27)) {
                                                        validate20.errors = [{ instancePath: instancePath + "/lifecycle/max_traces_per_pass", schemaPath: "#/$defs/lifecycle/properties/max_traces_per_pass/minimum", keyword: "minimum", params: { comparison: ">=", limit: 1 }, message: "must be >= 1" }];
                                                        return false;
                                                      }
                                                    }
                                                  }
                                                }
                                                var valid8 = _errs63 === errors;
                                              } else {
                                                var valid8 = true;
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
                              validate20.errors = [{ instancePath: instancePath + "/lifecycle", schemaPath: "#/$defs/lifecycle/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
                              return false;
                            }
                          }
                          var valid0 = _errs47 === errors;
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
        validate20.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    validate20.errors = vErrors;
    return errors === 0;
  }
  validate20.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };

  // ui/settings/config-validation.ts
  function validateLocalRuntimeConfig(value) {
    if (!validate_local_runtime_config_v5_default(value)) {
      return {
        valid: false,
        errors: (validate_local_runtime_config_v5_default.errors ?? []).map((error) => ({
          path: error.instancePath?.replace(/^\//, "").replaceAll("/", ".") ?? "",
          message: error.message ?? "\uD5C8\uC6A9 \uBC94\uC704\uB97C \uD655\uC778\uD558\uC138\uC694."
        }))
      };
    }
    const config = value;
    const { hot_days: hot, warm_days: warm, delete_after_days: expiry } = config.lifecycle;
    if (hot > warm) {
      return {
        valid: false,
        errors: [{ path: "lifecycle.warm_days", message: "Hot \uAE30\uC900\uC77C \uC774\uC0C1\uC774\uC5B4\uC57C \uD569\uB2C8\uB2E4." }]
      };
    }
    if (warm >= expiry) {
      return {
        valid: false,
        errors: [{ path: "lifecycle.delete_after_days", message: "Warm \uAE30\uC900\uC77C\uBCF4\uB2E4 \uCEE4\uC57C \uD569\uB2C8\uB2E4." }]
      };
    }
    return { valid: true, errors: [] };
  }

  // ui/settings/generated/validate-codex-integration-status-v1.js
  var validate_codex_integration_status_v1_default = validate21;
  var schema36 = { "title": "CodexIntegrationStatusV1", "type": "object", "additionalProperties": false, "required": ["schema_version", "config", "notify", "collector", "endpoint", "service", "data_retained", "collector_degradation_reasons"], "properties": { "schema_version": { "const": "codex_integration_status.v1" }, "config": { "$ref": "#/$defs/codex_connection_status" }, "notify": { "anyOf": [{ "$ref": "#/$defs/codex_notify_status" }, { "type": "null" }] }, "collector": { "$ref": "#/$defs/collector_status" }, "endpoint": { "type": ["string", "null"] }, "service": { "type": ["string", "null"] }, "data_retained": { "type": "boolean" }, "collector_degradation_reasons": { "type": "array", "maxItems": 3, "items": { "$ref": "#/$defs/collector_degradation_reason" } } }, "allOf": [{ "if": { "properties": { "collector": { "const": "degraded" } }, "required": ["collector"] }, "else": { "properties": { "collector_degradation_reasons": { "type": "array", "maxItems": 0 } } } }], "$defs": { "codex_connection_status": { "title": "CodexConnectionStatusV1", "type": "string", "enum": ["connected", "disconnected", "conflict"] }, "codex_notify_status": { "title": "CodexNotifyStatusV1", "type": "string", "enum": ["agentobs_owned", "external_preserved"] }, "collector_status": { "title": "CollectorStatusV1", "type": "string", "enum": ["ready", "degraded", "unavailable"] }, "collector_degradation_reason": { "title": "CollectorDegradationReasonV1", "type": "string", "enum": ["lifecycle_failure", "storage_pressure", "expired_trace"] } } };
  var schema37 = { "title": "CodexConnectionStatusV1", "type": "string", "enum": ["connected", "disconnected", "conflict"] };
  var schema38 = { "title": "CodexNotifyStatusV1", "type": "string", "enum": ["agentobs_owned", "external_preserved"] };
  var schema39 = { "title": "CollectorStatusV1", "type": "string", "enum": ["ready", "degraded", "unavailable"] };
  var schema40 = { "title": "CollectorDegradationReasonV1", "type": "string", "enum": ["lifecycle_failure", "storage_pressure", "expired_trace"] };
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
    const _errs2 = errors;
    let valid1 = true;
    const _errs3 = errors;
    if (data && typeof data == "object" && !Array.isArray(data)) {
      let missing0;
      if (data.collector === void 0 && (missing0 = "collector")) {
        const err0 = {};
        if (vErrors === null) {
          vErrors = [err0];
        } else {
          vErrors.push(err0);
        }
        errors++;
      } else {
        if (data.collector !== void 0) {
          if ("degraded" !== data.collector) {
            const err1 = {};
            if (vErrors === null) {
              vErrors = [err1];
            } else {
              vErrors.push(err1);
            }
            errors++;
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
    if (!_valid0) {
      const _errs5 = errors;
      if (data && typeof data == "object" && !Array.isArray(data)) {
        if (data.collector_degradation_reasons !== void 0) {
          let data1 = data.collector_degradation_reasons;
          const _errs6 = errors;
          if (errors === _errs6) {
            if (Array.isArray(data1)) {
              if (data1.length > 0) {
                validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons", schemaPath: "#/allOf/0/else/properties/collector_degradation_reasons/maxItems", keyword: "maxItems", params: { limit: 0 }, message: "must NOT have more than 0 items" }];
                return false;
              }
            } else {
              validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons", schemaPath: "#/allOf/0/else/properties/collector_degradation_reasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
              return false;
            }
          }
        }
      }
      var _valid0 = _errs5 === errors;
      valid1 = _valid0;
      if (valid1) {
        var props0 = {};
        props0.collector_degradation_reasons = true;
        props0.collector = true;
      }
    }
    if (!valid1) {
      const err2 = { instancePath, schemaPath: "#/allOf/0/if", keyword: "if", params: { failingKeyword: "else" }, message: 'must match "else" schema' };
      if (vErrors === null) {
        vErrors = [err2];
      } else {
        vErrors.push(err2);
      }
      errors++;
      validate21.errors = vErrors;
      return false;
    }
    if (errors === 0) {
      if (data && typeof data == "object" && !Array.isArray(data)) {
        let missing1;
        if (data.schema_version === void 0 && (missing1 = "schema_version") || data.config === void 0 && (missing1 = "config") || data.notify === void 0 && (missing1 = "notify") || data.collector === void 0 && (missing1 = "collector") || data.endpoint === void 0 && (missing1 = "endpoint") || data.service === void 0 && (missing1 = "service") || data.data_retained === void 0 && (missing1 = "data_retained") || data.collector_degradation_reasons === void 0 && (missing1 = "collector_degradation_reasons")) {
          validate21.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing1 }, message: "must have required property '" + missing1 + "'" }];
          return false;
        } else {
          const _errs8 = errors;
          for (const key0 in data) {
            if (!(key0 === "schema_version" || key0 === "config" || key0 === "notify" || key0 === "collector" || key0 === "endpoint" || key0 === "service" || key0 === "data_retained" || key0 === "collector_degradation_reasons")) {
              validate21.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
              return false;
              break;
            }
          }
          if (_errs8 === errors) {
            if (data.schema_version !== void 0) {
              const _errs9 = errors;
              if ("codex_integration_status.v1" !== data.schema_version) {
                validate21.errors = [{ instancePath: instancePath + "/schema_version", schemaPath: "#/properties/schema_version/const", keyword: "const", params: { allowedValue: "codex_integration_status.v1" }, message: "must be equal to constant" }];
                return false;
              }
              var valid4 = _errs9 === errors;
            } else {
              var valid4 = true;
            }
            if (valid4) {
              if (data.config !== void 0) {
                let data3 = data.config;
                const _errs10 = errors;
                if (typeof data3 !== "string") {
                  validate21.errors = [{ instancePath: instancePath + "/config", schemaPath: "#/$defs/codex_connection_status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                  return false;
                }
                if (!(data3 === "connected" || data3 === "disconnected" || data3 === "conflict")) {
                  validate21.errors = [{ instancePath: instancePath + "/config", schemaPath: "#/$defs/codex_connection_status/enum", keyword: "enum", params: { allowedValues: schema37.enum }, message: "must be equal to one of the allowed values" }];
                  return false;
                }
                var valid4 = _errs10 === errors;
              } else {
                var valid4 = true;
              }
              if (valid4) {
                if (data.notify !== void 0) {
                  let data4 = data.notify;
                  const _errs13 = errors;
                  const _errs14 = errors;
                  let valid6 = false;
                  const _errs15 = errors;
                  if (typeof data4 !== "string") {
                    const err3 = { instancePath: instancePath + "/notify", schemaPath: "#/$defs/codex_notify_status/type", keyword: "type", params: { type: "string" }, message: "must be string" };
                    if (vErrors === null) {
                      vErrors = [err3];
                    } else {
                      vErrors.push(err3);
                    }
                    errors++;
                  }
                  if (!(data4 === "agentobs_owned" || data4 === "external_preserved")) {
                    const err4 = { instancePath: instancePath + "/notify", schemaPath: "#/$defs/codex_notify_status/enum", keyword: "enum", params: { allowedValues: schema38.enum }, message: "must be equal to one of the allowed values" };
                    if (vErrors === null) {
                      vErrors = [err4];
                    } else {
                      vErrors.push(err4);
                    }
                    errors++;
                  }
                  var _valid1 = _errs15 === errors;
                  valid6 = valid6 || _valid1;
                  const _errs18 = errors;
                  if (data4 !== null) {
                    const err5 = { instancePath: instancePath + "/notify", schemaPath: "#/properties/notify/anyOf/1/type", keyword: "type", params: { type: "null" }, message: "must be null" };
                    if (vErrors === null) {
                      vErrors = [err5];
                    } else {
                      vErrors.push(err5);
                    }
                    errors++;
                  }
                  var _valid1 = _errs18 === errors;
                  valid6 = valid6 || _valid1;
                  if (!valid6) {
                    const err6 = { instancePath: instancePath + "/notify", schemaPath: "#/properties/notify/anyOf", keyword: "anyOf", params: {}, message: "must match a schema in anyOf" };
                    if (vErrors === null) {
                      vErrors = [err6];
                    } else {
                      vErrors.push(err6);
                    }
                    errors++;
                    validate21.errors = vErrors;
                    return false;
                  } else {
                    errors = _errs14;
                    if (vErrors !== null) {
                      if (_errs14) {
                        vErrors.length = _errs14;
                      } else {
                        vErrors = null;
                      }
                    }
                  }
                  var valid4 = _errs13 === errors;
                } else {
                  var valid4 = true;
                }
                if (valid4) {
                  if (data.collector !== void 0) {
                    let data5 = data.collector;
                    const _errs20 = errors;
                    if (typeof data5 !== "string") {
                      validate21.errors = [{ instancePath: instancePath + "/collector", schemaPath: "#/$defs/collector_status/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                      return false;
                    }
                    if (!(data5 === "ready" || data5 === "degraded" || data5 === "unavailable")) {
                      validate21.errors = [{ instancePath: instancePath + "/collector", schemaPath: "#/$defs/collector_status/enum", keyword: "enum", params: { allowedValues: schema39.enum }, message: "must be equal to one of the allowed values" }];
                      return false;
                    }
                    var valid4 = _errs20 === errors;
                  } else {
                    var valid4 = true;
                  }
                  if (valid4) {
                    if (data.endpoint !== void 0) {
                      let data6 = data.endpoint;
                      const _errs23 = errors;
                      if (typeof data6 !== "string" && data6 !== null) {
                        validate21.errors = [{ instancePath: instancePath + "/endpoint", schemaPath: "#/properties/endpoint/type", keyword: "type", params: { type: schema36.properties.endpoint.type }, message: "must be string,null" }];
                        return false;
                      }
                      var valid4 = _errs23 === errors;
                    } else {
                      var valid4 = true;
                    }
                    if (valid4) {
                      if (data.service !== void 0) {
                        let data7 = data.service;
                        const _errs25 = errors;
                        if (typeof data7 !== "string" && data7 !== null) {
                          validate21.errors = [{ instancePath: instancePath + "/service", schemaPath: "#/properties/service/type", keyword: "type", params: { type: schema36.properties.service.type }, message: "must be string,null" }];
                          return false;
                        }
                        var valid4 = _errs25 === errors;
                      } else {
                        var valid4 = true;
                      }
                      if (valid4) {
                        if (data.data_retained !== void 0) {
                          const _errs27 = errors;
                          if (typeof data.data_retained !== "boolean") {
                            validate21.errors = [{ instancePath: instancePath + "/data_retained", schemaPath: "#/properties/data_retained/type", keyword: "type", params: { type: "boolean" }, message: "must be boolean" }];
                            return false;
                          }
                          var valid4 = _errs27 === errors;
                        } else {
                          var valid4 = true;
                        }
                        if (valid4) {
                          if (data.collector_degradation_reasons !== void 0) {
                            let data9 = data.collector_degradation_reasons;
                            const _errs29 = errors;
                            if (errors === _errs29) {
                              if (Array.isArray(data9)) {
                                if (data9.length > 3) {
                                  validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons", schemaPath: "#/properties/collector_degradation_reasons/maxItems", keyword: "maxItems", params: { limit: 3 }, message: "must NOT have more than 3 items" }];
                                  return false;
                                } else {
                                  var valid9 = true;
                                  const len0 = data9.length;
                                  for (let i0 = 0; i0 < len0; i0++) {
                                    let data10 = data9[i0];
                                    const _errs31 = errors;
                                    if (typeof data10 !== "string") {
                                      validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons/" + i0, schemaPath: "#/$defs/collector_degradation_reason/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                                      return false;
                                    }
                                    if (!(data10 === "lifecycle_failure" || data10 === "storage_pressure" || data10 === "expired_trace")) {
                                      validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons/" + i0, schemaPath: "#/$defs/collector_degradation_reason/enum", keyword: "enum", params: { allowedValues: schema40.enum }, message: "must be equal to one of the allowed values" }];
                                      return false;
                                    }
                                    var valid9 = _errs31 === errors;
                                    if (!valid9) {
                                      break;
                                    }
                                  }
                                }
                              } else {
                                validate21.errors = [{ instancePath: instancePath + "/collector_degradation_reasons", schemaPath: "#/properties/collector_degradation_reasons/type", keyword: "type", params: { type: "array" }, message: "must be array" }];
                                return false;
                              }
                            }
                            var valid4 = _errs29 === errors;
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
      } else {
        validate21.errors = [{ instancePath, schemaPath: "#/type", keyword: "type", params: { type: "object" }, message: "must be object" }];
        return false;
      }
    }
    validate21.errors = vErrors;
    return errors === 0;
  }
  validate21.evaluated = { "props": true, "dynamicProps": false, "dynamicItems": false };

  // ui/settings/integration-status-validation.ts
  function validateCodexIntegrationStatus(value) {
    return validate_codex_integration_status_v1_default(value);
  }

  // ui/settings/generated/validate-codex-integration-error-v1.js
  var validate_codex_integration_error_v1_default = validate22;
  var schema41 = { "title": "CodexIntegrationErrorV1", "type": "object", "additionalProperties": false, "required": ["code", "message"], "properties": { "code": { "enum": ["integration_failed", "integration_connect_committed_unverified", "integration_disconnect_committed_unverified", "integration_settings_completed_unverified", "integration_outcome_uncertain"] }, "message": { "type": "string", "pattern": "^.{1,256}$" } } };
  var pattern4 = new RegExp("^.{1,256}$", "u");
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
        if (data.code === void 0 && (missing0 = "code") || data.message === void 0 && (missing0 = "message")) {
          validate22.errors = [{ instancePath, schemaPath: "#/required", keyword: "required", params: { missingProperty: missing0 }, message: "must have required property '" + missing0 + "'" }];
          return false;
        } else {
          const _errs1 = errors;
          for (const key0 in data) {
            if (!(key0 === "code" || key0 === "message")) {
              validate22.errors = [{ instancePath, schemaPath: "#/additionalProperties", keyword: "additionalProperties", params: { additionalProperty: key0 }, message: "must NOT have additional properties" }];
              return false;
              break;
            }
          }
          if (_errs1 === errors) {
            if (data.code !== void 0) {
              let data0 = data.code;
              const _errs2 = errors;
              if (!(data0 === "integration_failed" || data0 === "integration_connect_committed_unverified" || data0 === "integration_disconnect_committed_unverified" || data0 === "integration_settings_completed_unverified" || data0 === "integration_outcome_uncertain")) {
                validate22.errors = [{ instancePath: instancePath + "/code", schemaPath: "#/properties/code/enum", keyword: "enum", params: { allowedValues: schema41.properties.code.enum }, message: "must be equal to one of the allowed values" }];
                return false;
              }
              var valid0 = _errs2 === errors;
            } else {
              var valid0 = true;
            }
            if (valid0) {
              if (data.message !== void 0) {
                let data1 = data.message;
                const _errs3 = errors;
                if (errors === _errs3) {
                  if (typeof data1 === "string") {
                    if (!pattern4.test(data1)) {
                      validate22.errors = [{ instancePath: instancePath + "/message", schemaPath: "#/properties/message/pattern", keyword: "pattern", params: { pattern: "^.{1,256}$" }, message: 'must match pattern "^.{1,256}$"' }];
                      return false;
                    }
                  } else {
                    validate22.errors = [{ instancePath: instancePath + "/message", schemaPath: "#/properties/message/type", keyword: "type", params: { type: "string" }, message: "must be string" }];
                    return false;
                  }
                }
                var valid0 = _errs3 === errors;
              } else {
                var valid0 = true;
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

  // ui/settings/main.ts
  var fields = {
    "collection.file_reconcile_interval_ms": {
      path: "collection.file_reconcile_interval_ms",
      label: "\uD30C\uC77C \uD655\uC778 \uC8FC\uAE30",
      description: "\uC0C8 handoff \uD30C\uC77C\uC744 \uB2E4\uC2DC \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 1e3,
      max: 6e4,
      step: 1,
      unit: "ms",
      format: formatDuration
    },
    "collection.flush_interval_ms": {
      path: "collection.flush_interval_ms",
      label: "\uAE30\uB85D \uBC18\uC601 \uC8FC\uAE30",
      description: "\uD5C8\uC6A9\uB41C \uBC30\uCE58\uB97C durable storage\uC5D0 \uBC18\uC601\uD558\uB294 \uAC04\uACA9",
      min: 1e3,
      max: 6e4,
      step: 1,
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
      step: 1,
      unit: "bytes",
      format: formatBytes
    },
    "collection.active_heartbeat_interval_ms": {
      path: "collection.active_heartbeat_interval_ms",
      label: "\uD65C\uC131 heartbeat",
      description: "\uC791\uC5C5 \uC911 source \uC0C1\uD0DC\uB97C \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 3e4,
      max: 3e5,
      step: 1,
      unit: "ms",
      format: formatDuration
    },
    "collection.idle_heartbeat_interval_ms": {
      path: "collection.idle_heartbeat_interval_ms",
      label: "\uC720\uD734 heartbeat",
      description: "\uC791\uC5C5\uC774 \uC5C6\uC744 \uB54C source \uC0C1\uD0DC\uB97C \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 12e4,
      max: 9e5,
      step: 1,
      unit: "ms",
      format: formatDuration
    },
    "collection.local_storage_budget_bytes": {
      path: "collection.local_storage_budget_bytes",
      label: "\uB85C\uCEEC \uC800\uC7A5 \uD55C\uB3C4",
      description: "\uC218\uC9D1 \uB370\uC774\uD130\uAC00 \uC0AC\uC6A9\uD560 \uC218 \uC788\uB294 \uCD5C\uB300 \uB514\uC2A4\uD06C \uC608\uC0B0",
      min: 268435456,
      max: 21474836480,
      step: 1,
      unit: "bytes",
      format: formatBytes
    },
    "retention.max_record_age_days": {
      path: "retention.max_record_age_days",
      label: "\uC218\uB3D9 \uC815\uB9AC \uAE30\uC900\uC77C",
      description: "\uC790\uB3D9 \uC0AD\uC81C \uAE30\uC900\uACFC \uBCC4\uAC1C\uB85C, \uC774 \uAE30\uAC04\uBCF4\uB2E4 \uC624\uB798\uB41C trace\uB97C \uC218\uB3D9 \uC815\uB9AC \uB300\uC0C1\uC73C\uB85C \uC120\uD0DD",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "retention.max_archive_records": {
      path: "retention.max_archive_records",
      label: "\uC815\uB9AC \uB808\uCF54\uB4DC \uC0C1\uD55C",
      description: "\uD55C \uBC88\uC758 \uC218\uB3D9 \uC815\uB9AC \uC791\uC5C5\uC5D0\uC11C archive\uB85C \uC62E\uAE38 \uC218 \uC788\uB294 \uC804\uCCB4 \uB808\uCF54\uB4DC \uC0C1\uD55C",
      min: 1,
      max: 1e5,
      step: 1,
      unit: "records",
      format: (value) => `${formatNumber(value)}\uAC1C`
    },
    "retention.max_archive_bytes": {
      path: "retention.max_archive_bytes",
      label: "\uC815\uB9AC \uD06C\uAE30 \uC0C1\uD55C",
      description: "\uD55C \uBC88\uC758 \uC218\uB3D9 \uC815\uB9AC \uC791\uC5C5\uC5D0\uC11C \uC0DD\uC131\uD558\uB294 archive\uC758 \uC804\uCCB4 \uD06C\uAE30 \uC0C1\uD55C",
      min: 65536,
      max: 268435456,
      step: 1,
      unit: "bytes",
      format: formatBytes
    },
    "lifecycle.hot_days": {
      path: "lifecycle.hot_days",
      label: "Hot(\uCD5C\uADFC) \uAE30\uC900\uC77C",
      description: "\uC6D0\uBCF8 \uAD00\uCE21 \uC774\uB825\uC740 \uC81C\uAC70\uD558\uACE0 \uB9AC\uD3EC\uD2B8\uC6A9 \uAE30\uB85D\uC740 \uC720\uC9C0\uD558\uB294 Warm(\uC774\uB825 \uCD95\uC18C) \uB2E8\uACC4\uB85C \uC774\uB3D9",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "lifecycle.warm_days": {
      path: "lifecycle.warm_days",
      label: "Warm(\uC774\uB825 \uCD95\uC18C) \uAE30\uC900\uC77C",
      description: "\uC77C\uBC18 \uB9AC\uD3EC\uD2B8\uC5D0\uC11C \uC81C\uC678\uD558\uACE0 \uC555\uCD95\uD558\uC9C0 \uC54A\uC740 trace\uBCC4 JSON \uBB36\uC74C\uC73C\uB85C \uBCF4\uAD00\uD558\uB294 Cold(\uC7A5\uAE30 \uBCF4\uAD00) \uB2E8\uACC4\uB85C \uC774\uB3D9",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "lifecycle.delete_after_days": {
      path: "lifecycle.delete_after_days",
      label: "\uC644\uC804 \uC0AD\uC81C \uAE30\uC900\uC77C",
      description: "\uCD5C\uC2E0 \uAD00\uCE21 \uC774\uD6C4 \uAD00\uB9AC \uB300\uC0C1 trace\uAC00 \uC601\uAD6C \uC0AD\uC81C\uB418\uB294 \uC2DC\uC810",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "lifecycle.private_raw_days": {
      path: "lifecycle.private_raw_days",
      label: "\uC6D0\uBB38 \uC0C1\uC138 \uBCF4\uAD00",
      description: "\uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uCC45\uC744 \uCF30\uC744 \uB54C private \uC694\uCCAD\xB7\uC751\uB2F5 \uC6D0\uBB38 \uBCF4\uAD00 \uAE30\uAC04",
      min: 1,
      max: 3650,
      step: 1,
      unit: "days",
      format: (value) => `${formatNumber(value)}\uC77C`
    },
    "lifecycle.maintenance_interval_seconds": {
      path: "lifecycle.maintenance_interval_seconds",
      label: "\uC720\uC9C0\uAD00\uB9AC \uC8FC\uAE30",
      description: "\uB85C\uCEEC \uC218\uC9D1\uAE30\uAC00 \uB2E4\uC74C \uC815\uB9AC \uC791\uC5C5\uC744 \uD655\uC778\uD558\uB294 \uAC04\uACA9",
      min: 60,
      max: 86400,
      step: 1,
      unit: "seconds",
      format: formatDurationSeconds
    },
    "lifecycle.max_traces_per_pass": {
      path: "lifecycle.max_traces_per_pass",
      label: "\uC815\uB9AC \uC791\uC5C5\uB2F9 trace",
      description: "\uD55C \uBC88\uC758 \uC815\uB9AC \uC791\uC5C5\uC5D0\uC11C \uCC98\uB9AC\uD560 \uCD5C\uB300 trace \uC218",
      min: 1,
      max: 128,
      step: 1,
      unit: "traces",
      format: (value) => `${formatNumber(value)}\uAC1C`
    }
  };
  var rootElement = document.querySelector("#app");
  if (!(rootElement instanceof HTMLDivElement)) throw new Error("settings root is missing");
  var app = rootElement;
  var SESSION_TOKEN_KEY = "agent-observability.settings.session.v1";
  var INITIAL_INTEGRATION_RETRY_MS = 1500;
  var fragmentToken = new URLSearchParams(location.hash.slice(1)).get("session") ?? "";
  var token = fragmentToken || readSessionToken();
  if (fragmentToken) writeSessionToken(fragmentToken);
  history.replaceState(null, "", `${location.pathname}${location.search}`);
  var persisted = null;
  var draft = null;
  var defaults = null;
  var revision = "";
  var integration = null;
  var integrationUnavailable = false;
  var integrationRequestGeneration = 0;
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
  window.addEventListener("beforeunload", (event) => {
    if (!isDirty()) return;
    event.preventDefault();
    event.returnValue = "";
  });
  window.addEventListener("focus", () => {
    void refreshIntegrationStatus();
  });
  void bootstrap();
  async function bootstrap() {
    renderLoading();
    if (!token) {
      renderExpired();
      return;
    }
    try {
      const envelope = await api("/api/config");
      applyEnvelope(envelope);
      let shouldRenderSettings = false;
      try {
        shouldRenderSettings = await loadInitialIntegrationStatus();
      } catch (error) {
        const apiError = error;
        if (apiError.code === "invalid_session") throw error;
        integration = null;
        integrationUnavailable = true;
        shouldRenderSettings = true;
      }
      if (!token) return;
      if (shouldRenderSettings) renderSettings();
      heartbeatTimer ??= window.setInterval(() => void heartbeat(), 2e4);
    } catch (error) {
      const apiError = error;
      if (apiError.code === "invalid_session" || apiError.code === "network_failure") {
        expireSession();
      } else {
        renderUnavailable(messageOf(error));
      }
    }
  }
  async function loadInitialIntegrationStatus() {
    const generation = ++integrationRequestGeneration;
    try {
      const initial = await integrationApi("/api/integrations/codex");
      const next = initial.config === "connected" && initial.collector === "unavailable" ? await new Promise((resolve) => window.setTimeout(resolve, INITIAL_INTEGRATION_RETRY_MS)).then(() => integrationApi("/api/integrations/codex")) : initial;
      if (generation !== integrationRequestGeneration || !token) return false;
      integration = next;
      integrationUnavailable = false;
      return true;
    } catch (error) {
      if (generation !== integrationRequestGeneration) return false;
      throw error;
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
    <p>\uD130\uBBF8\uB110\uC5D0\uC11C <code>agentobs settings</code>\uB97C \uC2E4\uD589\uD574 \uC0C8 \uC138\uC158\uC744 \uC5EC\uC138\uC694.</p>
  </main>`;
    mountIcons();
  }
  function renderSettings(focusTarget) {
    if (!draft) return;
    app.innerHTML = `<div class="app-shell">
    <header class="topbar">
      <div class="brand"><span class="brand-mark"><i data-lucide="settings-2"></i></span><span>Agent Observability</span></div>
      <div class="topbar-actions">
        <button class="button monitor-button" id="open-dashboard" type="button"><i data-lucide="monitor-up"></i>\uBAA8\uB2C8\uD130\uB9C1</button>
        <span class="session-badge"><i data-lucide="shield-check"></i>\uB85C\uCEEC \uC804\uC6A9 \xB7 \uC138\uC158 \uD65C\uC131</span>
        <button class="icon-button" id="close-session" type="button" title="\uC124\uC815 \uC138\uC158 \uB2EB\uAE30" aria-label="\uC124\uC815 \uC138\uC158 \uB2EB\uAE30"><i data-lucide="x"></i></button>
      </div>
    </header>
    <div class="workspace">
      <nav class="section-nav" aria-label="\uC124\uC815 \uC601\uC5ED">
        <p class="nav-label">\uC124\uC815</p>
        <a href="#overview" class="active" aria-current="page"><i data-lucide="gauge"></i>\uAC1C\uC694</a>
        <a href="#collection"><i data-lucide="activity"></i>\uC218\uC9D1</a>
        <a href="#privacy"><i data-lucide="shield-check"></i>\uAC1C\uC778\uC815\uBCF4</a>
        <a href="#storage"><i data-lucide="database"></i>\uC800\uC7A5\uC18C</a>
        <a href="#lifecycle"><i data-lucide="heart-pulse"></i>\uB370\uC774\uD130 \uBCF4\uAD00</a>
        <a href="#retention"><i data-lucide="archive"></i>\uC218\uB3D9 \uC815\uB9AC</a>
        <div class="nav-note"><strong>Codex</strong><span>${configNavigationStatus()}</span><span>${collectorNavigationStatus()}</span></div>
      </nav>
      <main class="settings-main">
        <form id="settings-form" novalidate>
          ${overviewSection(draft)}
          ${collectionSection(draft)}
          ${privacySection(draft)}
          ${storageSection(draft)}
          ${lifecycleSection(draft)}
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
      <p class="dialog-error" id="close-error" role="alert"></p>
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
    ${integrationPanel()}
    <div class="policy-strip" aria-label="\uC815\uCC45 \uC694\uC57D">
      ${summaryItem("activity", "\uD655\uC778 \uC8FC\uAE30", formatDuration(config.collection.file_reconcile_interval_ms))}
      ${summaryItem("sliders-horizontal", "\uBC30\uCE58 \uC0C1\uD55C", `${formatNumber(config.collection.max_batch_records)}\uAC1C`)}
      ${summaryItem("database", "\uC800\uC7A5 \uD55C\uB3C4", storage)}
      ${summaryItem("archive", "\uBCF4\uAD00 \uAE30\uAC04", `${formatNumber(config.retention.max_record_age_days)}\uC77C`)}
    </div>
    <div class="policy-notice"><i data-lucide="shield-check"></i><div><strong>\uC774 \uD654\uBA74\uC740 \uB85C\uCEEC \uC815\uCC45\uB9CC \uBCC0\uACBD\uD569\uB2C8\uB2E4.</strong><span>\uC678\uBD80 \uC804\uC1A1 \uC5C6\uC774 Rust\uAC00 \uAC80\uC99D\uD55C \uB4A4 private config\uC5D0 \uC6D0\uC790\uC801\uC73C\uB85C \uC800\uC7A5\uD569\uB2C8\uB2E4.</span></div></div>
  </section>`;
  }
  function integrationPanel() {
    const connected = integration?.config === "connected";
    const ready = integration?.collector === "ready";
    const degraded = integration?.collector === "degraded";
    const conflicted2 = integration?.config === "conflict";
    const degradedCopy = integrationDegradedCopy(
      integration?.collector_degradation_reasons ?? []
    );
    const state = integrationUnavailable ? "\uC0C1\uD0DC \uD655\uC778 \uBD88\uAC00" : conflicted2 ? "\uC124\uC815 \uCDA9\uB3CC" : connected && degraded ? degradedCopy.state : connected && ready ? "\uC218\uC9D1 \uC911" : connected ? "\uC218\uC9D1\uAE30 \uC751\uB2F5 \uC5C6\uC74C" : "\uC5F0\uACB0 \uC548 \uB428";
    const detail = integrationUnavailable ? "Codex \uC0C1\uD0DC\uB97C \uD655\uC778\uD560 \uB54C\uAE4C\uC9C0 \uC5F0\uACB0 \uBCC0\uACBD\uC744 \uC7A0\uAC14\uC2B5\uB2C8\uB2E4. \uB2E4\uC2DC \uD655\uC778\uC744 \uB20C\uB7EC \uC0C1\uD0DC\uB97C \uC870\uD68C\uD574 \uC8FC\uC138\uC694." : conflicted2 ? "Codex \uC124\uC815\uC774 \uC5F0\uACB0 \uD6C4 \uBCC0\uACBD\uB418\uC5B4 \uC790\uB3D9 \uBCF5\uC6D0\uC744 \uC911\uB2E8\uD588\uC2B5\uB2C8\uB2E4." : connected && degraded ? degradedCopy.detail : connected && ready ? "Codex \uC774\uBCA4\uD2B8\uB97C private local runtime\uC5D0 \uBC18\uC601\uD569\uB2C8\uB2E4." : connected ? "Codex \uC5F0\uACB0\uC740 \uC720\uC9C0\uB418\uC9C0\uB9CC \uB85C\uCEEC \uC218\uC9D1\uAE30\uC5D0 \uC5F0\uACB0\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4." : "Codex \uC790\uB3D9 \uC218\uC9D1\uC744 \uC5F0\uACB0\uD558\uBA74 \uB2E4\uC74C \uC791\uC5C5\uBD80\uD130 \uAE30\uB85D\uD569\uB2C8\uB2E4.";
    const action = integrationUnavailable ? `<button class="button secondary" id="refresh-integration" type="button"><i data-lucide="refresh-cw"></i>\uB2E4\uC2DC \uD655\uC778</button>` : connected ? `<button class="button secondary" id="toggle-integration" type="button"><i data-lucide="power"></i>\uC5F0\uACB0 \uD574\uC81C</button>` : `<button class="button primary" id="toggle-integration" type="button"><i data-lucide="cable"></i>Codex \uC5F0\uACB0</button>`;
    const panelState = integrationUnavailable ? "unavailable" : conflicted2 ? "conflict" : degraded ? "degraded" : ready ? "ready" : "idle";
    const collectorLabel = integrationUnavailable ? "\uD655\uC778 \uBD88\uAC00" : degraded ? "\uC0C1\uD0DC \uC800\uD558" : ready ? "\uC815\uC0C1" : "\uC911\uC9C0";
    return `<div class="integration-panel" data-state="${panelState}" data-config-state="${integration?.config ?? "disconnected"}" data-collector-state="${integration?.collector ?? "unavailable"}">
    <div class="integration-identity"><span class="integration-icon"><i data-lucide="activity"></i></span><div><span>Codex</span><strong>${state}</strong><small>${detail}</small></div></div>
    <div class="integration-meta"><span><b>\uC218\uC9D1\uAE30</b>${collectorLabel}</span><span><b>\uC800\uC7A5</b>\uB85C\uCEEC \uC804\uC6A9</span>${integration?.endpoint ? `<span class="endpoint"><b>Endpoint</b>${escapeHtml(integration.endpoint)}</span>` : ""}</div>
    <div class="integration-actions">${action}<button class="button monitor-button" id="overview-dashboard" type="button"><i data-lucide="external-link"></i>\uB9AC\uD3EC\uD2B8 \uC5F4\uAE30</button></div>
  </div>`;
  }
  function integrationDegradedCopy(reasons) {
    if (reasons.length === 0) {
      return {
        state: "\uC218\uC9D1\uAE30 \uC0C1\uD0DC \uC800\uD558",
        detail: "\uB9AC\uD3EC\uD2B8 \uBC18\uC601 \uB610\uB294 \uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uB9AC\uAC00 \uC9C0\uC5F0\uB420 \uC218 \uC788\uC2B5\uB2C8\uB2E4."
      };
    }
    const labels = {
      lifecycle_failure: "\uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uB9AC \uBBF8\uC644\uB8CC",
      storage_pressure: "\uC815\uB9AC\uC6A9 \uC784\uC2DC \uC800\uC7A5 \uACF5\uAC04 \uBD80\uC871",
      expired_trace: "\uB9CC\uB8CC\uB41C \uC138\uC158 \uB370\uC774\uD130 \uC81C\uC678"
    };
    const details = {
      lifecycle_failure: "\uC77C\uBD80 \uB370\uC774\uD130 \uB610\uB294 \uC624\uB958\uB85C \uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uB9AC \uC791\uC5C5\uC744 \uC644\uB8CC\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4.",
      storage_pressure: "\uC815\uB9AC \uC791\uC5C5\uC5D0 \uD544\uC694\uD55C \uC784\uC2DC \uC800\uC7A5 \uACF5\uAC04\uC774 \uBD80\uC871\uD574 \uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uB9AC\uAC00 \uC9C0\uC5F0\uB429\uB2C8\uB2E4.",
      expired_trace: "\uC644\uC804\uD788 \uB9CC\uB8CC\uB41C \uC138\uC158\uC758 \uD6C4\uC18D \uB370\uC774\uD130\uAC00 \uC81C\uC678\uB418\uC5C8\uC2B5\uB2C8\uB2E4. \uD574\uB2F9 \uC791\uC5C5\uC744 \uACC4\uC18D \uAE30\uB85D\uD558\uB824\uBA74 \uC5D0\uC774\uC804\uD2B8\uC5D0\uC11C \uC0C8 \uC138\uC158\uC744 \uC2DC\uC791\uD574\uC57C \uD569\uB2C8\uB2E4."
    };
    const reasonOrder = [
      "lifecycle_failure",
      "storage_pressure",
      "expired_trace"
    ];
    const orderedReasons = reasonOrder.filter((reason) => reasons.includes(reason));
    return {
      state: orderedReasons.map((reason) => labels[reason]).join(" \xB7 "),
      detail: orderedReasons.map((reason) => details[reason]).join(" ")
    };
  }
  function configNavigationStatus() {
    if (integrationUnavailable) return "\uC790\uB3D9 \uC218\uC9D1 \uC0C1\uD0DC \uD655\uC778 \uBD88\uAC00";
    if (integration?.config === "connected") return "\uC790\uB3D9 \uC218\uC9D1 \uC5F0\uACB0\uB428";
    if (integration?.config === "conflict") return "Codex \uC124\uC815 \uCDA9\uB3CC";
    return "\uC790\uB3D9 \uC218\uC9D1 \uC5F0\uACB0 \uC548 \uB428";
  }
  function collectorNavigationStatus() {
    if (integrationUnavailable) return "collector \uC0C1\uD0DC \uD655\uC778 \uBD88\uAC00";
    if (integration?.collector === "ready") return "collector \uC2E4\uD589 \uC911";
    if (integration?.collector === "degraded") return "collector \uC2E4\uD589 \uC911 \xB7 \uC0C1\uD0DC \uC800\uD558";
    return "collector \uC911\uC9C0\uB428";
  }
  function collectionSection(config) {
    return `<section class="settings-section" id="collection" aria-labelledby="collection-title">
    ${sectionTitle("collection", "activity", "\uC218\uC9D1", "\uD30C\uC77C \uD655\uC778\uACFC durable \uAE30\uB85D \uBC18\uC601 \uAC04\uACA9")}
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
    ${sectionTitle("storage", "database", "\uC800\uC7A5\uC18C", "\uB85C\uCEEC \uB370\uC774\uD130\uAC00 \uB118\uC9C0 \uBABB\uD558\uB294 \uB514\uC2A4\uD06C \uC608\uC0B0")}
    <div class="section-grid">
      <div class="field-grid single">${fieldControl(fields["collection.local_storage_budget_bytes"], config)}</div>
      ${singleRuler("storage-visual", "\uC124\uC815 \uC800\uC7A5 \uD55C\uB3C4", fields["collection.local_storage_budget_bytes"], "256 MiB", "20 GiB", true, "\uD604\uC7AC \uC0AC\uC6A9\uB7C9\uC774 \uC544\uB2CC \uD5C8\uC6A9 \uD55C\uB3C4")}
    </div>
  </section>`;
  }
  function privacySection(config) {
    const enabled = config.capture_private_codex_turn_details ?? false;
    return `<section class="settings-section" id="privacy" aria-labelledby="privacy-title">
    <div class="section-title"><span class="section-icon"><i data-lucide="shield-check"></i></span><div><h2 id="privacy-title">\uAC1C\uC778\uC815\uBCF4</h2><p>Codex \uC791\uC5C5 \uACBD\uB85C\uC640 \uB300\uD654 \uB0B4\uC6A9\uC744 \uBCC4\uB3C4 \uB85C\uCEEC \uC0C1\uC138 \uC800\uC7A5\uC18C\uC5D0 \uBCF4\uAD00\uD560\uC9C0 \uC120\uD0DD\uD569\uB2C8\uB2E4.</p></div></div>
    <label class="collection-toggle privacy-toggle" data-boolean-field="capture_private_codex_turn_details">
      <span><strong>\uC694\uCCAD\xB7\uC751\uB2F5 \uC0C1\uC138 \uC800\uC7A5</strong><small id="private-details-copy">${enabled ? "\uC0C8 Codex turn\uC758 \uACBD\uB85C\uC640 \uC694\uCCAD\xB7\uC751\uB2F5\uC744 \uB85C\uCEEC\uC5D0 \uC800\uC7A5\uD569\uB2C8\uB2E4" : "\uAEBC\uC9D0 \xB7 \uC77C\uBC18 \uC9C0\uD45C\uC640 \uD574\uC2DC \uC2DD\uBCC4\uC790\uB9CC \uC800\uC7A5\uD569\uB2C8\uB2E4"}</small></span>
      <input type="checkbox" id="capture-private-codex-turn-details" ${enabled ? "checked" : ""}>
      <span class="toggle-track" aria-hidden="true"><span></span></span>
    </label>
    <div class="privacy-warning"><i data-lucide="shield-check"></i><div><strong>\uBA85\uC2DC\uC801\uC73C\uB85C \uCF20 \uC774\uD6C4\uC758 \uC0C8 turn\uBD80\uD130 \uC801\uC6A9\uB429\uB2C8\uB2E4.</strong><span>\uC6D0\uBB38\uC740 team \uC804\uC1A1\xB7\uC77C\uBC18 \uB9AC\uD3EC\uD2B8\xB7export\uC5D0 \uD3EC\uD568\uB418\uC9C0 \uC54A\uC73C\uBA70, \uC774 Mac\uC758 private localhost \uC0C1\uC138 \uD654\uBA74\uC5D0\uC11C\uB9CC \uC694\uCCAD\uD560 \uB54C \uC77D\uC2B5\uB2C8\uB2E4. \uBBFC\uAC10\uC815\uBCF4\uAC00 \uD3EC\uD568\uB420 \uC218 \uC788\uC2B5\uB2C8\uB2E4.</span></div></div>
  </section>`;
  }
  function lifecycleSection(config) {
    const enabled = config.lifecycle.enabled;
    return `<section class="settings-section" id="lifecycle" aria-labelledby="lifecycle-title">
    <div class="section-title"><span class="section-icon"><i data-lucide="heart-pulse"></i></span><div><h2 id="lifecycle-title">\uB370\uC774\uD130 \uBCF4\uAD00 \uC815\uCC45</h2><p>\uCD5C\uC2E0 trace \uAD00\uCE21 \uC2DC\uC810\uBD80\uD130 \uB204\uC801\uB41C \uACBD\uACFC \uAE30\uAC04\uC73C\uB85C Hot(\uCD5C\uADFC) \u2192 Warm(\uC774\uB825 \uCD95\uC18C) \u2192 Cold(\uC7A5\uAE30 \uBCF4\uAD00) \u2192 Delete(\uC0AD\uC81C)\uB97C \uC801\uC6A9\uD569\uB2C8\uB2E4.</p></div></div>
    <label class="collection-toggle lifecycle-toggle" data-boolean-field="lifecycle.enabled">
      <span><strong>\uC790\uB3D9 \uC815\uB9AC</strong><small id="lifecycle-enabled-copy">${enabled ? "\uCF1C\uC9D0 \xB7 \uB2E4\uC74C \uC815\uB9AC \uC791\uC5C5\uBD80\uD130 \uBCF4\uAD00 \uAE30\uC900\uC744 \uC9C0\uB09C \uAE30\uC874 \uB370\uC774\uD130\uC5D0\uB3C4 \uC801\uC6A9\uB429\uB2C8\uB2E4" : "\uAEBC\uC9D0 \xB7 \uAE30\uC874 \uC218\uB3D9 \uBCF4\uAD00 \uC124\uC815\uACFC \uC6D0\uBB38 \uBCF4\uAD00 \uB3D9\uC791\uC744 \uC720\uC9C0\uD569\uB2C8\uB2E4"}</small></span>
      <input type="checkbox" id="lifecycle-enabled" ${enabled ? "checked" : ""}>
      <span class="toggle-track" aria-hidden="true"><span></span></span>
    </label>
    <div class="section-grid lifecycle-grid">
      <div class="field-grid">${fieldControl(fields["lifecycle.hot_days"], config)}${fieldControl(fields["lifecycle.warm_days"], config)}${fieldControl(fields["lifecycle.delete_after_days"], config)}${fieldControl(fields["lifecycle.private_raw_days"], config)}${fieldControl(fields["lifecycle.maintenance_interval_seconds"], config)}${fieldControl(fields["lifecycle.max_traces_per_pass"], config)}</div>
      ${lifecycleTimeline(config)}
    </div>
    <div class="retention-note lifecycle-warning" role="note"><i data-lucide="archive"></i><span><strong>\uC0AD\uC81C\uB294 \uB418\uB3CC\uB9B4 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.</strong> \uC790\uB3D9 \uC815\uB9AC\uB97C \uCF1C\uAC70\uB098 \uAE30\uC900\uC77C\uC744 \uC904\uC774\uBA74 \uBCF4\uAD00 \uAE30\uC900\uC744 \uC9C0\uB09C \uAE30\uC874 \uB370\uC774\uD130\uAC00 \uB2E4\uC74C \uC815\uB9AC \uC791\uC5C5\uC5D0\uC11C \uC774\uB3D9\uD558\uAC70\uB098 \uC601\uAD6C \uC0AD\uC81C\uB420 \uC218 \uC788\uC2B5\uB2C8\uB2E4. \uC644\uC804\uD788 \uC0AD\uC81C\uB41C \uC138\uC158\uC758 \uC0C8 \uD65C\uB3D9\uC744 \uC218\uC9D1\uD558\uB824\uBA74 \uC5D0\uC774\uC804\uD2B8\uC5D0\uC11C \uC0C8 \uC138\uC158\uC744 \uC2DC\uC791\uD574\uC57C \uD569\uB2C8\uB2E4. \uC124\uC815 \uC800\uC7A5 \uC644\uB8CC\uB294 \uC815\uB9AC \uC2E4\uD589\uC774\uB098 \uB514\uC2A4\uD06C \uACF5\uAC04 \uD68C\uC218\uB97C \uC758\uBBF8\uD558\uC9C0 \uC54A\uC2B5\uB2C8\uB2E4.</span></div>
  </section>`;
  }
  function lifecycleTimeline(config) {
    return `<figure class="policy-visual timeline lifecycle-timeline" data-min="1" data-max="3650" data-log="true">
    <figcaption><span>\uB204\uC801 \uACBD\uACFC \uAE30\uAC04</span><strong data-lifecycle-value>Hot(\uCD5C\uADFC) \u2192 Warm(\uC774\uB825 \uCD95\uC18C) \u2192 Cold(\uC7A5\uAE30 \uBCF4\uAD00) \u2192 Delete(\uC0AD\uC81C)</strong></figcaption>
    <div class="timeline-track" aria-hidden="true">
      <span class="timeline-marker first" data-marker data-path="lifecycle.hot_days"><b>Warm ${config.lifecycle.hot_days}\uC77C</b></span>
      <span class="timeline-marker second" data-marker data-path="lifecycle.warm_days"><b>Cold ${config.lifecycle.warm_days}\uC77C</b></span>
      <span class="timeline-marker third" data-marker data-path="lifecycle.delete_after_days"><b>Delete ${config.lifecycle.delete_after_days}\uC77C</b></span>
    </div>
    <div class="ruler-labels"><span>\uCD5C\uC2E0 \uAD00\uCE21</span><span>10\uB144</span></div>
    <p>\uAC01 \uAC12\uC740 \uB2E8\uACC4\uBCC4 \uCD94\uAC00 \uAE30\uAC04\uC774 \uC544\uB2C8\uB77C \uCD5C\uC2E0 trace \uAD00\uCE21 \uC774\uD6C4\uC758 \uB204\uC801 \uACBD\uACFC \uAE30\uAC04\uC785\uB2C8\uB2E4.</p>
  </figure>`;
  }
  function retentionSection(config) {
    return `<section class="settings-section" id="retention" aria-labelledby="retention-title">
    ${sectionTitle("retention", "archive", "\uC218\uB3D9 \uC815\uB9AC", "\uC790\uB3D9 \uC0AD\uC81C\uC640 \uBCC4\uAC1C\uC778 \uC218\uB3D9 \uB300\uC0C1 \uAE30\uC900 \uBC0F \uC791\uC5C5\uBCC4 archive \uC0C1\uD55C")}
    <div class="section-grid">
      <div class="field-grid">${fieldControl(fields["retention.max_record_age_days"], config)}${fieldControl(fields["retention.max_archive_records"], config)}${fieldControl(fields["retention.max_archive_bytes"], config)}</div>
      <div class="visual-stack">
        ${singleRuler("retention-visual", "\uC218\uB3D9 \uC815\uB9AC \uAE30\uC900\uC77C", fields["retention.max_record_age_days"], "1\uC77C", "10\uB144", true, "\uAE30\uC900\uC77C\uBCF4\uB2E4 \uC624\uB798\uB41C trace\uB294 \uC218\uB3D9 \uC815\uB9AC \uB300\uC0C1")}
        ${singleRuler("archive-records-visual", "\uC791\uC5C5\uBCC4 Archive \uB808\uCF54\uB4DC \uC0C1\uD55C", fields["retention.max_archive_records"], "1", "100k", true)}
        ${singleRuler("archive-bytes-visual", "\uC791\uC5C5\uBCC4 Archive \uD06C\uAE30 \uC0C1\uD55C", fields["retention.max_archive_bytes"], "64 KiB", "256 MiB", true)}
      </div>
    </div>
    <div class="retention-note"><i data-lucide="archive"></i><span>\uC218\uB3D9 \uC815\uB9AC \uAE30\uC900 ${config.retention.max_record_age_days}\uC77C\uC740 \uC790\uB3D9 \uC644\uC804 \uC0AD\uC81C \uAE30\uC900 ${config.lifecycle.delete_after_days}\uC77C\uACFC \uBCC4\uAC1C\uC785\uB2C8\uB2E4. \uB808\uCF54\uB4DC\xB7\uD06C\uAE30 \uC0C1\uD55C\uC740 \uD55C \uBC88\uC758 \uC218\uB3D9 \uC815\uB9AC \uC791\uC5C5\uC5D0 \uD568\uAED8 \uC801\uC6A9\uB429\uB2C8\uB2E4. \uAE30\uC900\uC77C\uC744 \uC904\uC5EC\uB3C4 \uC989\uC2DC \uC0AD\uC81C\uD558\uC9C0 \uC54A\uC73C\uBA70 \uBCC4\uB3C4\uC758 \uACC4\uD68D \uC0DD\uC131\xB7\uC801\uC6A9 \uC808\uCC28\uB97C \uB530\uB985\uB2C8\uB2E4.</span></div>
  </section>`;
  }
  function sectionTitle(id, icon, title, description) {
    return `<div class="section-title"><span class="section-icon"><i data-lucide="${icon}"></i></span><div><h2 id="${id}-title">${title}</h2><p>${description}</p></div></div>`;
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
    return `<figure class="policy-visual timeline" id="${id}" data-log="${logarithmic}" data-first-label="${firstLabel}" data-second-label="${secondLabel}"${sharedScale}>
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
    document.querySelector("#capture-private-codex-turn-details")?.addEventListener("change", handlePrivateDetails);
    document.querySelector("#lifecycle-enabled")?.addEventListener("change", handleLifecycleEnabled);
    document.querySelector("#discard")?.addEventListener("click", discardChanges);
    document.querySelector("#reset")?.addEventListener("click", openResetDialog);
    document.querySelector("#cancel-reset")?.addEventListener("click", closeResetDialog);
    document.querySelector("#confirm-reset")?.addEventListener("click", resetDefaults);
    document.querySelector("#close-session")?.addEventListener("click", requestCloseSession);
    document.querySelector("#cancel-close")?.addEventListener("click", closeCloseDialog);
    document.querySelector("#confirm-close")?.addEventListener("click", () => void closeSession());
    document.querySelector("#toggle-integration")?.addEventListener("click", () => void toggleIntegration());
    document.querySelector("#refresh-integration")?.addEventListener("click", () => void refreshIntegration());
    document.querySelector("#open-dashboard")?.addEventListener("click", () => void openDashboard());
    document.querySelector("#overview-dashboard")?.addEventListener("click", () => void openDashboard());
    document.querySelectorAll("dialog").forEach((dialog) => {
      dialog.addEventListener("keydown", trapDialogFocus);
    });
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
  async function toggleIntegration() {
    if (busy || !integration || integrationUnavailable) return;
    const lifecycleToken = token;
    const generation = ++integrationRequestGeneration;
    busy = true;
    setBusy(true);
    try {
      const method = integration.config === "connected" ? "DELETE" : "POST";
      const nextIntegration = await integrationApi("/api/integrations/codex", { method });
      if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
      integration = nextIntegration;
      integrationUnavailable = false;
      busy = false;
      renderSettings("toggle-integration");
      showToast(
        integration.config === "connected" ? "Codex \uC790\uB3D9 \uC218\uC9D1\uC744 \uC5F0\uACB0\uD588\uC2B5\uB2C8\uB2E4." : "Codex \uC790\uB3D9 \uC218\uC9D1\uC744 \uD574\uC81C\uD588\uC2B5\uB2C8\uB2E4.",
        "success"
      );
    } catch (error) {
      if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
      integration = null;
      integrationUnavailable = true;
      try {
        const next = await integrationApi("/api/integrations/codex");
        if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
        integration = next;
        integrationUnavailable = false;
      } catch (statusError) {
        if (token !== lifecycleToken || generation !== integrationRequestGeneration) return;
        if (statusError.code === "invalid_session") {
          busy = false;
          expireSession();
          return;
        }
      }
      busy = false;
      renderSettings(integrationUnavailable ? "refresh-integration" : "toggle-integration");
      showToast(`${messageOf(error)} ${integrationUnavailable ? "\uC0C1\uD0DC\uB97C \uD655\uC778\uD560 \uC218 \uC5C6\uC5B4 \uBCC0\uACBD\uC744 \uC7A0\uAC14\uC2B5\uB2C8\uB2E4. \uB2E4\uC2DC \uD655\uC778\uC744 \uB20C\uB7EC \uC8FC\uC138\uC694." : "\uD604\uC7AC \uC0C1\uD0DC\uB97C \uB2E4\uC2DC \uD655\uC778\uD588\uC2B5\uB2C8\uB2E4."}`, "error");
    }
  }
  async function refreshIntegration() {
    if (busy) return;
    const generation = ++integrationRequestGeneration;
    busy = true;
    setBusy(true);
    try {
      const next = await integrationApi("/api/integrations/codex");
      if (generation !== integrationRequestGeneration || !token) return;
      integration = next;
      integrationUnavailable = false;
      busy = false;
      renderSettings("toggle-integration");
      showToast("Codex \uC790\uB3D9 \uC218\uC9D1 \uC0C1\uD0DC\uB97C \uD655\uC778\uD588\uC2B5\uB2C8\uB2E4.", "success");
    } catch (error) {
      if (generation !== integrationRequestGeneration || !token) return;
      busy = false;
      const apiError = error;
      if (apiError.code === "invalid_session") {
        expireSession();
        return;
      }
      integration = null;
      integrationUnavailable = true;
      renderSettings("refresh-integration");
      showToast(messageOf(error), "error");
    }
  }
  async function refreshIntegrationStatus() {
    if (busy || !persisted || !token) return;
    const generation = ++integrationRequestGeneration;
    const previous = integration;
    const wasUnavailable = integrationUnavailable;
    try {
      const next = await integrationApi("/api/integrations/codex");
      if (!token || generation !== integrationRequestGeneration) return;
      integration = next;
      integrationUnavailable = false;
      if (wasUnavailable || !sameIntegrationStatus(previous, next)) {
        renderSettings();
      }
    } catch (error) {
      if (generation !== integrationRequestGeneration) return;
      const apiError = error;
      if (apiError.code === "invalid_session") {
        expireSession();
        return;
      }
      integration = null;
      integrationUnavailable = true;
      if (!wasUnavailable || previous !== null) renderSettings();
    }
  }
  function sameIntegrationStatus(left, right) {
    const rightReasons = right.collector_degradation_reasons;
    return left !== null && left.config === right.config && left.collector === right.collector && left.endpoint === right.endpoint && left.service === right.service && left.data_retained === right.data_retained && left.collector_degradation_reasons.length === right.collector_degradation_reasons.length && left.collector_degradation_reasons.every((reason) => rightReasons.includes(reason));
  }
  async function openDashboard() {
    if (busy) return;
    try {
      await api("/api/dashboard/open", { method: "POST" });
      showToast("\uBAA8\uB2C8\uD130\uB9C1 \uB9AC\uD3EC\uD2B8\uB97C \uC5F4\uC5C8\uC2B5\uB2C8\uB2E4.", "success");
    } catch (error) {
      showToast(messageOf(error), "error");
    }
  }
  function trapDialogFocus(event) {
    if (event.key !== "Tab") return;
    const dialog = event.currentTarget;
    if (!(dialog instanceof HTMLDialogElement) || !dialog.open) return;
    const controls = Array.from(
      dialog.querySelectorAll(
        "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])"
      )
    );
    if (controls.length === 0) return;
    const first = controls[0];
    const last = controls.at(-1);
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !dialog.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
      event.preventDefault();
      first.focus();
    }
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
  function handlePrivateDetails(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || !draft) return;
    draft.capture_private_codex_turn_details = input.checked;
    setText(
      "private-details-copy",
      input.checked ? "\uC0C8 Codex turn\uC758 \uACBD\uB85C\uC640 \uC694\uCCAD\xB7\uC751\uB2F5\uC744 \uB85C\uCEEC\uC5D0 \uC800\uC7A5\uD569\uB2C8\uB2E4" : "\uAEBC\uC9D0 \xB7 \uC77C\uBC18 \uC9C0\uD45C\uC640 \uD574\uC2DC \uC2DD\uBCC4\uC790\uB9CC \uC800\uC7A5\uD569\uB2C8\uB2E4"
    );
    updateDirtyState();
  }
  function handleLifecycleEnabled(event) {
    const input = event.target;
    if (!(input instanceof HTMLInputElement) || !draft) return;
    draft.lifecycle.enabled = input.checked;
    setText(
      "lifecycle-enabled-copy",
      input.checked ? "\uCF1C\uC9D0 \xB7 \uB2E4\uC74C \uC815\uB9AC \uC791\uC5C5\uBD80\uD130 \uBCF4\uAD00 \uAE30\uC900\uC744 \uC9C0\uB09C \uAE30\uC874 \uB370\uC774\uD130\uC5D0\uB3C4 \uC801\uC6A9\uB429\uB2C8\uB2E4" : "\uAEBC\uC9D0 \xB7 \uAE30\uC874 \uC218\uB3D9 \uBCF4\uAD00 \uC124\uC815\uACFC \uC6D0\uBB38 \uBCF4\uAD00 \uB3D9\uC791\uC744 \uC720\uC9C0\uD569\uB2C8\uB2E4"
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
      const label = marker.querySelector("b");
      if (label && path.startsWith("lifecycle.")) {
        const stage = path === "lifecycle.hot_days" ? "Warm" : path === "lifecycle.warm_days" ? "Cold" : "Delete";
        label.textContent = `${stage} ${getValue(draft, path)}\uC77C`;
      }
    });
    document.querySelectorAll("[data-dual-value]").forEach((output) => {
      const visual = output.closest(".policy-visual");
      const paths = Array.from(visual?.querySelectorAll("[data-path]") ?? []).map(
        (item) => item.dataset.path
      );
      const labels = [visual?.dataset.firstLabel ?? "\uCCAB \uBC88\uC9F8", visual?.dataset.secondLabel ?? "\uB450 \uBC88\uC9F8"];
      output.textContent = paths.map((path, index) => `${labels[index]} ${fields[path].format(getValue(draft, path))}`).join(" \xB7 ");
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
    const booleanChanges = booleanChangeCount(draft, persisted);
    const dirty = booleanChanges > 0 || changed.length > 0;
    document.querySelector("#save-band")?.classList.toggle("dirty", dirty);
    setText("save-title", conflicted ? "\uC678\uBD80 \uBCC0\uACBD \uAC10\uC9C0" : dirty ? `${changed.length + booleanChanges}\uAC1C \uBCC0\uACBD` : "\uC800\uC7A5\uB428");
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
    const validation = validateLocalRuntimeConfig(draft);
    if (!validation.valid) {
      for (const error of validation.errors) {
        const path = error.path;
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
        try {
          await rebaseDraftOnLatest();
          showToast("\uCD5C\uC2E0 \uC124\uC815\uC744 \uBD88\uB7EC\uC640 \uB0B4 \uBCC0\uACBD\uB9CC \uB2E4\uC2DC \uC801\uC6A9\uD588\uC2B5\uB2C8\uB2E4. \uAC80\uD1A0 \uD6C4 \uC800\uC7A5\uD558\uC138\uC694.", "error");
        } catch (rebaseError) {
          const rebaseApiError = rebaseError;
          if (rebaseApiError.code === "invalid_session" || rebaseApiError.code === "network_failure") {
            expireSession();
            return;
          }
          showToast("\uCD5C\uC2E0 \uC124\uC815\uC744 \uBD88\uB7EC\uC624\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4. \uD3B8\uC9D1\uAC12\uC740 \uC720\uC9C0\uB429\uB2C8\uB2E4. \uB2E4\uC2DC \uC800\uC7A5\uD574 \uC7AC\uC2DC\uB3C4\uD558\uC138\uC694.", "error");
        }
      } else if (apiError.code === "invalid_session" || apiError.code === "network_failure") {
        expireSession();
        return;
      } else {
        showToast("\uC124\uC815\uC744 \uC800\uC7A5\uD558\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4. reason=" + (apiError.code ?? "request_failed"), "error");
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
    const privateDetailsChanged = (localDraft.capture_private_codex_turn_details ?? false) !== (localBase.capture_private_codex_turn_details ?? false);
    const lifecycleEnabledChanged = localDraft.lifecycle.enabled !== localBase.lifecycle.enabled;
    const latest = await api("/api/config");
    applyEnvelope(latest);
    if (!draft) return;
    for (const path of changed) setValue(draft, path, getValue(localDraft, path));
    if (enabledChanged) draft.enabled = localDraft.enabled;
    if (privateDetailsChanged) {
      draft.capture_private_codex_turn_details = localDraft.capture_private_codex_turn_details ?? false;
    }
    if (lifecycleEnabledChanged) draft.lifecycle.enabled = localDraft.lifecycle.enabled;
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
    if (!defaults || !draft) return;
    draft = { ...structuredClone(defaults), storage_budget: structuredClone(draft.storage_budget) };
    closeResetDialog();
    renderSettings("reset");
    showToast("\uAE30\uBCF8\uAC12\uC744 \uD3B8\uC9D1\uAC12\uC5D0 \uC801\uC6A9\uD588\uC2B5\uB2C8\uB2E4. \uC800\uC7A5\uD574\uC57C \uBC18\uC601\uB429\uB2C8\uB2E4.", "neutral");
  }
  async function closeSession() {
    if (busy) return;
    busy = true;
    setBusy(true);
    setText("close-error", "");
    try {
      await api("/api/shutdown", { method: "POST" });
      if (persisted) draft = structuredClone(persisted);
      conflicted = false;
      expireSession();
    } catch (error) {
      const apiError = error;
      if (apiError.code === "invalid_session") {
        if (persisted) draft = structuredClone(persisted);
        conflicted = false;
        expireSession();
        return;
      }
      setText(
        "close-error",
        "\uC138\uC158\uC744 \uB2EB\uC9C0 \uBABB\uD588\uC2B5\uB2C8\uB2E4. \uB85C\uCEEC process \uC5F0\uACB0\uC744 \uD655\uC778\uD558\uACE0 \uB2E4\uC2DC \uC2DC\uB3C4\uD558\uC138\uC694."
      );
      document.querySelector("#confirm-close")?.focus();
    } finally {
      busy = false;
      if (token) {
        setBusy(false);
        updateDirtyState();
      }
    }
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
      await refreshIntegrationStatus();
    } catch {
      expireSession();
    }
  }
  function expireSession() {
    window.clearInterval(heartbeatTimer);
    token = "";
    clearSessionToken();
    renderExpired();
  }
  function readSessionToken() {
    try {
      return sessionStorage.getItem(SESSION_TOKEN_KEY) ?? "";
    } catch {
      return "";
    }
  }
  function writeSessionToken(value) {
    try {
      sessionStorage.setItem(SESSION_TOKEN_KEY, value);
    } catch {
    }
  }
  function clearSessionToken() {
    try {
      sessionStorage.removeItem(SESSION_TOKEN_KEY);
    } catch {
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
      draft && persisted && (booleanChangeCount(draft, persisted) > 0 || changedPaths(draft, persisted).length > 0)
    );
  }
  async function api(path, init = {}) {
    const headers = new Headers(init.headers);
    headers.set("x-agent-observability-session", token);
    let response;
    try {
      response = await fetch(path, { ...init, headers, cache: "no-store" });
    } catch {
      const error = new Error("\uB85C\uCEEC \uC124\uC815 process\uC5D0 \uC5F0\uACB0\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4.");
      error.code = "network_failure";
      throw error;
    }
    if (!response.ok) {
      const body = await response.json().catch(() => ({}));
      const error = new Error(body.message ?? `\uC694\uCCAD\uC774 \uC2E4\uD328\uD588\uC2B5\uB2C8\uB2E4 (${response.status}).`);
      if (body.code) error.code = body.code;
      throw error;
    }
    if (response.status === 204) return void 0;
    return await response.json();
  }
  async function integrationApi(path, init = {}) {
    let value;
    try {
      value = await api(path, { ...init, signal: AbortSignal.timeout(5e3) });
    } catch (error) {
      const failure = error;
      if (failure.code?.startsWith("integration_") && !validate_codex_integration_error_v1_default({ code: failure.code, message: failure.message })) {
        throw new Error("Codex \uBCC0\uACBD \uACB0\uACFC \uC751\uB2F5\uC744 \uD655\uC778\uD560 \uC218 \uC5C6\uC2B5\uB2C8\uB2E4. \uC0C1\uD0DC\uB97C \uB2E4\uC2DC \uD655\uC778\uD574\uC57C \uD569\uB2C8\uB2E4.");
      }
      throw error;
    }
    if (!validateCodexIntegrationStatus(value)) {
      throw new Error("Codex \uC790\uB3D9 \uC218\uC9D1 \uC0C1\uD0DC \uC751\uB2F5\uC774 \uC62C\uBC14\uB974\uC9C0 \uC54A\uC2B5\uB2C8\uB2E4.");
    }
    return value;
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
    if (kind !== "error") {
      window.setTimeout(() => toast.classList.remove("visible"), 4e3);
    }
  }
  function mountIcons() {
    createIcons({
      icons: {
        Activity,
        Archive,
        Cable,
        Check,
        Database,
        ExternalLink,
        Gauge,
        HeartPulse,
        MonitorUp,
        Power,
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
  function booleanChangeCount(left, right) {
    return Number(left.enabled !== right.enabled) + Number(
      (left.capture_private_codex_turn_details ?? false) !== (right.capture_private_codex_turn_details ?? false)
    ) + Number(left.lifecycle.enabled !== right.lifecycle.enabled);
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
  function formatDurationSeconds(value) {
    if (value >= 3600 && value % 3600 === 0) return `${formatNumber(value / 3600)}\uC2DC\uAC04`;
    if (value >= 60 && value % 60 === 0) return `${formatNumber(value / 60)}\uBD84`;
    return `${formatNumber(value)}\uCD08`;
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
  function escapeHtml(value) {
    const entities = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;"
    };
    return value.replace(/[&<>"']/g, (character) => entities[character] ?? character);
  }
  function setDisabled(id, value) {
    const button = document.querySelector(`#${id}`);
    if (button) button.disabled = value;
  }
  function messageOf(error) {
    return error instanceof Error ? error.message : "\uC54C \uC218 \uC5C6\uB294 \uC624\uB958\uAC00 \uBC1C\uC0DD\uD588\uC2B5\uB2C8\uB2E4.";
  }
})();
