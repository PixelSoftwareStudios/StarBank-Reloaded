import type { XmlComparison } from "./transport";

export function XmlComparisonView({
  comparison,
  beforeTitle,
  afterTitle,
  matchingText,
}: {
  comparison: XmlComparison;
  beforeTitle: string;
  afterTitle: string;
  matchingText: string;
}) {
  return (
    <div className="mt-3">
      <p role="status">
        {comparison.firstDifferenceLine === null
          ? matchingText
          : `First XML difference at line ${comparison.firstDifferenceLine}. Both excerpts start at line ${comparison.startLine}.`}
      </p>
      <div className="mt-2 grid min-w-0 gap-2 sm:grid-cols-2">
        <Excerpt title={beforeTitle} excerpt={comparison.before} />
        <Excerpt title={afterTitle} excerpt={comparison.after} />
      </div>
    </div>
  );
}

function Excerpt({
  title,
  excerpt,
}: {
  title: string;
  excerpt: XmlComparison["before"];
}) {
  return (
    <div className="min-w-0 rounded border border-neutral-400 p-2 dark:border-neutral-600">
      <h3 className="mb-1 font-semibold">{title}</h3>
      {excerpt.omitsStart && <p className="text-xs">Earlier text omitted</p>}
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">
        {excerpt.text}
      </pre>
      {excerpt.omitsEnd && <p className="text-xs">Later text omitted</p>}
    </div>
  );
}
