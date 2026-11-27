import type { BatchBackupReview } from "./transport";
import { RecordedPath } from "./write-recovery-details";

const reviewSummary = {
  snapshot_commit_uncertain: "StarBank could not verify the backup write.",
  metadata_rejected:
    "StarBank could not record the backup as verified because the metadata was rejected.",
  metadata_commit_uncertain:
    "StarBank could not verify the backup metadata write.",
} satisfies Record<BatchBackupReview["kind"], string>;

export function BatchBackupReviewDetails({
  review,
}: {
  review: BatchBackupReview;
}) {
  return (
    <div className="space-y-3">
      <p>{reviewSummary[review.kind]}</p>
      <p className="text-muted">
        These locations were recorded during the backup attempt. Their current
        status is unknown.
      </p>
      <RecordedPath label="Recorded backup location" path={review.backup} />
      {review.recovery ? (
        <RecordedPath
          label="Recorded recovery location"
          path={review.recovery}
        />
      ) : (
        <p>No recovery location was recorded.</p>
      )}
      <div>
        <h4 className="font-medium">Reason reported</h4>
        <p className="whitespace-pre-wrap wrap-anywhere">{review.reason}</p>
        {review.reasonTruncated && (
          <p className="text-muted">The reported reason was shortened.</p>
        )}
      </div>
    </div>
  );
}
