#!/usr/bin/env bash
# Build (if needed) the reference leekscript fat jar, compile this runner, execute a snippet like the
# Java JUnit harness (LeekScript.compileSnippet → init → staticInit → runIA → export).
#
# Requires: bash, JDK `java` + `javac` on PATH (optional: JAVA_HOME overrides both),
#           leek-wars-generator Gradle wrapper (for the jar).
#
# Examples:
#   echo 'return 1 + 2;' | ./scripts/parity/java_snippet.sh --version 4 --from-stdin
#   ./scripts/parity/java_snippet.sh --version 4 --code 'return true;'
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
JAR="$ROOT/leek-wars-generator/leekscript/leekscript.jar"
RUNNER_SRC="$ROOT/tools/parity_java_runner/src/main/java/leekscript/parity/ParitySnippetRunner.java"
OUT_DIR="$ROOT/target/parity_java_runner_classes"

if [[ -n "${JAVA_HOME:-}" ]]; then
	JAVAC="$JAVA_HOME/bin/javac"
	JAVA="$JAVA_HOME/bin/java"
else
	JAVAC="$(command -v javac 2>/dev/null || true)"
	JAVA="$(command -v java 2>/dev/null || true)"
fi
if [[ -z "$JAVAC" || ! -x "$JAVAC" ]]; then
	echo "java_snippet.sh: need an executable javac (JDK on PATH, or set JAVA_HOME)" >&2
	exit 1
fi
if [[ -z "$JAVA" || ! -x "$JAVA" ]]; then
	echo "java_snippet.sh: need an executable java (JDK on PATH, or set JAVA_HOME)" >&2
	exit 1
fi

if [[ ! -f "$JAR" ]]; then
	(
		cd "$ROOT/leek-wars-generator"
		./gradlew :leekscript:jar -q
)
fi

if [[ ! -f "$JAR" ]]; then
	echo "java_snippet.sh: missing fat jar at $JAR (run leek-wars-generator :leekscript:jar)" >&2
	exit 1
fi

mkdir -p "$OUT_DIR"
# Toolchain matches leek-wars-generator/leekscript/build.gradle (Java 25).
"$JAVAC" --release 25 -cp "$JAR" -d "$OUT_DIR" "$RUNNER_SRC"

RES="${LEEKSCRIPT_TEST_RESOURCES:-$ROOT/leek-wars-generator/leekscript/src/test/resources}"
RUN_IN_RES=0
for a in "$@"; do
	if [[ "$a" == "--file" ]]; then
		RUN_IN_RES=1
		break
	fi
done
if [[ "$RUN_IN_RES" -eq 1 ]]; then
	cd "$RES"
fi
exec "$JAVA" -cp "$OUT_DIR:$JAR" leekscript.parity.ParitySnippetRunner "$@"
