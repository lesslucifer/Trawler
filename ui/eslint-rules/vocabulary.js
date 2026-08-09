import { arbitraryValue, classify, collectClasses, isBareSize, opacitySteps } from "./classes.js";

const PRAGMA = /design-allow:\s*(\S+)\s*(?:—|--)\s*(\S.*?)\s*$/;

export default {
  meta: {
    type: "problem",
    docs: {
      description:
        "Every class must resolve to a token in the closed Tailwind vocabulary (E-DFLOW.W6, DEC-17). Tailwind drops an unknown utility silently; this rule is the alarm.",
    },
    schema: [],
    messages: {
      offlist:
        "`{{cls}}` is not in the closed vocabulary — Tailwind emits no CSS for it and the element renders unstyled. Use a token from ui/tailwind.config.js.",
      arbitrary:
        "`{{cls}}` is an arbitrary value. Closing the config cannot reach arbitrary values, so this is the only check that can: name `{{value}}` as a token in ui/tailwind.config.js.",
      modifier:
        "`{{cls}}` dims with `/{{value}}`, which is not a step in the closed opacity scale ({{steps}}).",
      unresolvable:
        "`{{cls}}` is assembled from runtime fragments, so its vocabulary cannot be checked. Interpolate whole class names instead.",
      pragmaMalformed:
        "A design-allow pragma reads `// design-allow: <class> — <reason>`; this one gives no reason.",
      pragmaStale: "The pragma allows `{{cls}}`, which is not on the line below it.",
      pragmaOverSize:
        "The pragma covers template shapes only, never a size. `{{cls}}` carries a value — name it as a token in ui/tailwind.config.js.",
    },
  },

  create(context) {
    const sourceCode = context.sourceCode ?? context.getSourceCode();
    const allowed = new Map();

    for (const comment of sourceCode.getAllComments()) {
      if (!comment.value.includes("design-allow:")) continue;
      const match = PRAGMA.exec(comment.value);
      if (!match) {
        context.report({ node: comment, messageId: "pragmaMalformed" });
        continue;
      }
      const [, cls] = match;
      const value = arbitraryValue(cls);
      if (value === null || isBareSize(value)) {
        context.report({ node: comment, messageId: "pragmaOverSize", data: { cls } });
        continue;
      }
      const next = sourceCode.lines[comment.loc.start.line] ?? "";
      if (!next.includes(cls)) {
        context.report({ node: comment, messageId: "pragmaStale", data: { cls } });
        continue;
      }
      allowed.set(`${comment.loc.start.line + 1}:${cls}`, true);
    }

    return {
      "Program:exit"(program) {
        for (const { cls, node, offset, partial } of collectClasses(program)) {
          const loc = {
            start: sourceCode.getLocFromIndex(offset),
            end: sourceCode.getLocFromIndex(offset + cls.length),
          };
          if (partial) {
            context.report({ node, loc, messageId: "unresolvable", data: { cls } });
            continue;
          }
          const verdict = classify(cls);
          if (verdict.legal) continue;
          if (allowed.has(`${loc.start.line}:${cls}`)) continue;
          context.report({
            node,
            loc,
            messageId: verdict.kind,
            data: { cls, value: verdict.value, steps: opacitySteps().join(", ") },
          });
        }
      },
    };
  },
};
