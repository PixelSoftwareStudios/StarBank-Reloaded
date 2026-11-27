import { numericFormatter } from "react-number-format";

const GROUP_SIZE = 3;
const MAX_FORMATTER_LENGTH = 256;
const DISPLAY_LEXEME = /^-?[0-9]+(?:\.[0-9]+)?$/;

export function groupNumberLexeme(type: string, value: string): string {
  if ((type !== "int" && type !== "fixed") || !DISPLAY_LEXEME.test(value)) {
    return value;
  }

  const firstDigit = value.startsWith("-") ? 1 : 0;
  const decimalPoint = value.indexOf(".");
  const integerEnd = decimalPoint < 0 ? value.length : decimalPoint;
  if (integerEnd - firstDigit <= GROUP_SIZE) return value;

  // React Number Format treats leading zeroes differently. The fallback also
  // bounds its grouping work and processes longer lexemes in one pass.
  if (value[firstDigit] !== "0" && value.length <= MAX_FORMATTER_LENGTH) {
    return numericFormatter(value, { thousandSeparator: "," });
  }

  const groups: string[] = [];
  for (let end = integerEnd; end > firstDigit; end -= GROUP_SIZE) {
    groups.push(value.slice(Math.max(firstDigit, end - GROUP_SIZE), end));
  }
  return (
    value.slice(0, firstDigit) +
    groups.reverse().join(",") +
    value.slice(integerEnd)
  );
}
