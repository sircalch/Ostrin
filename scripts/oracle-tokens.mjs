// Token rules of the oracle files (tests/oracle/*-expected.txt), shared by every checker that compares printed output with
// them. A token is one of:
//   v|t     the printed number must be within t of v (absolute);
//   <=b     the printed number must be at most b;
//   a|b     the printed token must equal a or b (when neither side is numeric);
//   text    the printed token must equal text exactly.
// Lines are compared one by one, with the same number of tokens on each line.

export const oracleTokenMatches = (expected, actual) => {
  if (expected.startsWith("<=")) {
    const bound = Number(expected.slice(2));
    const x = Number(actual);
    return actual !== "" && Number.isFinite(x) && x <= bound;
  }
  if (expected.includes("|")) {
    const [left, right] = expected.split("|");
    const value = Number(left);
    const tolerance = Number(right);
    if (left !== "" && right !== "" && Number.isFinite(value) && Number.isFinite(tolerance)) {
      const x = Number(actual);
      return actual !== "" && Number.isFinite(x) && Math.abs(x - value) <= tolerance;
    }
    return actual === left || actual === right;
  }
  return expected === actual;
};

// Compares printed text with an expected text, line by line. Returns { ok, lines, failure }.
export function compareLines(actualText, expectedText) {
  const actual = actualText.split("\n").filter((line) => line !== "");
  const expected = expectedText.split("\n").filter((line) => line !== "");
  if (actual.length !== expected.length) {
    return { ok: false, lines: expected.length, failure: `line count ${actual.length} vs ${expected.length}` };
  }
  for (let index = 0; index < expected.length; index += 1) {
    const got = actual[index].split(" ");
    const want = expected[index].split(" ");
    if (got.length !== want.length) return { ok: false, lines: expected.length, failure: `line ${index}: token count` };
    for (let t = 0; t < want.length; t += 1) {
      if (!oracleTokenMatches(want[t], got[t])) {
        return { ok: false, lines: expected.length, failure: `line ${index}: token ${JSON.stringify(got[t])} does not match ${JSON.stringify(want[t])}` };
      }
    }
  }
  return { ok: true, lines: expected.length };
}
