# Table cell crop artifact lifecycle

Cell crops are disabled by default and are never embedded into the public AST.
Persistence requires `CropPrivacyConsent::ExplicitDebugOrBenchmark` plus an
explicitly configured artifact directory.

Stored PNGs are content-addressed. The accompanying diagnostic contains only
the artifact reference, crop hash, source-image hash, bounds, and content
reference; recognized cell text is not copied into the routing decision.
Retention is bounded by file count and cleanup removes the oldest artifacts.

Applications are responsible for presenting consent, selecting a private
directory, enforcing any additional time-based retention policy, and deleting
the directory after debugging or benchmark intake. Production release profiles
must leave the crop store unset.

