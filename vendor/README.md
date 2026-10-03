This contains external packages not under the license of Xee.

Each directory is a copy of a W3C test-suite repository at the upstream
commit recorded below. The commits were identified by comparing the git
blob hash of every file with the upstream history. The `filters` file in
each directory is Xee's own (the tests expected to fail) and not part of
the upstream suite.

To refresh a copy, replace it with the upstream tree at a newer commit,
leave out the same files, and record the new commit here. A refresh moves
conformance verdicts by construction, so it is a change of its own.

## qt3tests (`xpath-tests/`)

This is a copy of the XPath and XQuery test suite, hosted at
https://github.com/w3c/qt3tests, at upstream commit
`f8987877e22fad89589dcc956a12ec528e45b8eb` (2024-05-16). Xee commit
`cebfd9c3` copied it.

Xee uses these tests to verify the correctness of its XPath implementation.

Some files and directories that do not contain tests have been removed in this
copy to save space:

```
drivers
releases
reports
ReportingResults
ReportingResults31
results
tools
viewer
xqueryx.zip
```

Four files differ from upstream only by a final newline, which upstream
lacks: `op/base64Binary-less-than.xml`, `op/divide-dayTimeDuration.xml`,
`prod/GeneralComp.ge.xml` and `prod/TreatExpr.xml`.

## xslt30-test (`xslt-tests/`)

This is a copy of the XSLT 3.0 test suite, hosted at
https://github.com/w3c/xslt30-test, at upstream commit
`1fcf15b0b18bfbd2afde5947ab3fc7f51a4ba0bd` (2023-11-17). Xee commit
`23e26ced` copied it.

Xee uses these tests to verify its XSLT implementation. The copy is
complete: no upstream file was removed or changed.
