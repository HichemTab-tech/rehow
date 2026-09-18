import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex, utf8ToBytes } from "@noble/hashes/utils.js";

const encoder = new TextEncoder();
const DOMAIN = encoder.encode("rehow-guide-verification-v1\0");
const FIELDS = [
  "body",
  "title",
  "summary",
  "applies_to",
  "prerequisites",
  "warnings",
  "files_affected",
  "related_guide_ids",
  "references",
  "attachments",
];

const text = (value) => ({ kind: "string", value });
const integer = (digits, negative = false) => ({
  kind: "integer",
  digits,
  negative,
});
const finite = (coefficient, exponent = "0", negative = false) => ({
  kind: "float",
  coefficient,
  exponent,
  negative,
});
const sequence = (...values) => ({ kind: "sequence", values });
const mapping = (...entries) => ({ kind: "mapping", entries });

const vectors = [
  {
    name: "minimal",
    body: "## Steps\n\nRun it.\n",
    fields: {
      title: text("Install tool"),
      summary: text("Install the downloaded tool"),
    },
    unknown: [],
    expected:
      "0966e3140e5e3a51d09c341a6322cf751ec7be076021cbd724bd2362b84dcd54",
  },
  {
    name: "unknown mapping and negative zero value",
    body: "Body\n",
    fields: {
      title: text("Install tool"),
      summary: text("Install the downloaded tool"),
    },
    unknown: [
      [
        "extension",
        mapping(
          [text("alpha"), integer("1")],
          [text("zero"), finite("0", "0", true)],
        ),
      ],
    ],
    expected:
      "70e8fae2032f7c823a1f687f1d42ac5dbd82d52245122272faf86f52e2b43786",
  },
  {
    name: "nested known extension and sorted unknown roots",
    body: "Body\n",
    fields: {
      title: text("Install tool"),
      summary: text("Install the downloaded tool"),
      applies_to: sequence(
        mapping(
          [text("platform"), text("Linux")],
          [text("future"), text("one")],
        ),
      ),
    },
    unknown: [
      ["zeta", integer("1")],
      ["alpha", integer("2")],
    ],
    expected:
      "a122dc67943fc3c5a0bc438b5e1089512c5f8b672117ca21d35300cbd460c30b",
  },
];

function u64(value) {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false);
  return bytes;
}

function concat(...parts) {
  const length = parts.reduce((total, part) => total + part.length, 0);
  const output = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function frame(bytes) {
  return concat(u64(bytes.length), bytes);
}

function compareBytes(left, right) {
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return left.length - right.length;
}

function encodeValue(value, mappingKey = false) {
  switch (value.kind) {
    case "string":
      return concat(Uint8Array.of(0x04), frame(encoder.encode(value.value)));
    case "integer":
      return concat(
        Uint8Array.of(0x02, Number(value.negative)),
        frame(encoder.encode(value.digits)),
      );
    case "float": {
      const negative =
        value.negative && !(mappingKey && value.coefficient === "0");
      return concat(
        Uint8Array.of(0x03, 0x00, Number(negative)),
        frame(encoder.encode(value.coefficient)),
        Uint8Array.of(Number(value.exponent.startsWith("-"))),
        frame(encoder.encode(value.exponent.replace(/^-/, ""))),
      );
    }
    case "sequence": {
      const children = value.values.map((child) =>
        frame(encodeValue(child, mappingKey)),
      );
      return concat(Uint8Array.of(0x05), u64(children.length), ...children);
    }
    case "mapping": {
      const entries = value.entries
        .map(([key, child]) => [
          encodeValue(key, true),
          encodeValue(child, mappingKey),
        ])
        .sort(([leftKey, leftValue], [rightKey, rightValue]) => {
          const keyOrder = compareBytes(leftKey, rightKey);
          return keyOrder === 0
            ? compareBytes(leftValue, rightValue)
            : keyOrder;
        });
      return concat(
        Uint8Array.of(0x06),
        u64(entries.length),
        ...entries.flatMap(([key, child]) => [frame(key), frame(child)]),
      );
    }
    default:
      throw new Error(`unsupported vector value: ${value.kind}`);
  }
}

function preimage(vector) {
  const parts = [DOMAIN, u64(FIELDS.length)];
  for (const field of FIELDS) {
    parts.push(frame(encoder.encode(field)));
    if (field === "body") {
      parts.push(Uint8Array.of(1), frame(encoder.encode(vector.body)));
    } else if (Object.hasOwn(vector.fields, field)) {
      parts.push(Uint8Array.of(1), frame(encodeValue(vector.fields[field])));
    } else {
      parts.push(Uint8Array.of(0));
    }
  }
  const unknown = vector.unknown.toSorted(([left], [right]) =>
    compareBytes(encoder.encode(left), encoder.encode(right)),
  );
  parts.push(u64(unknown.length));
  for (const [key, value] of unknown) {
    parts.push(frame(encoder.encode(key)), frame(encodeValue(value)));
  }
  return concat(...parts);
}

if (
  bytesToHex(blake3(utf8ToBytes(""))) !==
  "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
) {
  throw new Error(
    "independent BLAKE3 implementation failed its published empty-input vector",
  );
}

let failed = false;
for (const vector of vectors) {
  const actual = bytesToHex(blake3(preimage(vector)));
  if (actual !== vector.expected) {
    failed = true;
    console.error(
      `${vector.name}: expected ${vector.expected}, received ${actual}`,
    );
  }
}
if (failed) process.exitCode = 1;
