# 0.79.44 compositor integration

2026-09-21: ./Build.sh render-test exited 0 (79 existing cases). Windows uses
Build.bat render-test. The existing occlusion case compares a 256-value opaque
palette against the full color pipeline, with 4096 direct pixels. Existing
damage, linear alpha, ICC, shared CPU/HDR windows and transport remain covered.
The same suite verifies warm native assets require no repeated uploads.
Actual bootfb/Virtio Desktop evidence: Distribution/Tests/Reports/0.79.44;
scope and costs: workspace Docs/Desktop/GrafikIntegration07944.txt/.json.
