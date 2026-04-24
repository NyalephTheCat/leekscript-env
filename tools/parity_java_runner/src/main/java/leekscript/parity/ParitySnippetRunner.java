package leekscript.parity;

import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Locale;

import leekscript.compiler.LeekScript;
import leekscript.compiler.Options;
import leekscript.compiler.exceptions.LeekCompilerException;
import leekscript.runner.AI;
import leekscript.runner.LeekRunException;

/**
 * CLI helper for Rust parity / benchmarks: compile → init → staticInit → runIA → {@link AI#export}.
 * <p>
 * Stdout: final exported result string only (no trailing newline). Stderr: one * {@code leek_bench_iter} line per repetition with timings. Exit 0 on success.
 */
public final class ParitySnippetRunner {

	private ParitySnippetRunner() {}

	private static long fnv1a64(byte[] bytes) {
		long h = 0xcbf29ce484222325L;
		for (byte b : bytes) {
			h ^= (b & 0xFF);
			h *= 0x100000001b3L;
		}
		return h;
	}

	public static void main(String[] args) {
		try {
			run(args);
		} catch (LeekCompilerException e) {
			System.err.println("leek_bench_error phase=compile ref=" + e.getError().name());
			e.printStackTrace(System.err);
			System.exit(2);
		} catch (LeekRunException e) {
			System.err.println("leek_bench_error phase=run ref=" + e.getError().name());
			e.printStackTrace(System.err);
			System.exit(3);
		} catch (Exception e) {
			System.err.println("leek_bench_error phase=internal ref=UNKNOWN");
			e.printStackTrace(System.err);
			System.exit(1);
		}
	}

	private static void run(String[] args) throws Exception {
		int version = 4;
		boolean strict = false;
		String code = null;
		String file = null;
		boolean stdin = false;
		int repeat = 1;
		for (int i = 0; i < args.length; i++) {
			switch (args[i]) {
				case "--version" -> version = Integer.parseInt(args[++i]);
				case "--strict" -> strict = true;
				case "--code" -> code = args[++i];
				case "--file" -> file = args[++i];
				case "--from-stdin" -> stdin = true;
				case "--repeat" -> repeat = Math.max(1, Integer.parseInt(args[++i]));
				default -> throw new IllegalArgumentException("unknown arg: " + args[i]);
			}
		}
		if (stdin) {
			code = new String(System.in.readAllBytes(), StandardCharsets.UTF_8);
		}
		if (file == null && code == null) {
			throw new IllegalArgumentException("need --code, --file, or --from-stdin");
		}
		if (file != null && code != null) {
			throw new IllegalArgumentException("use either --file or --code/--from-stdin, not both");
		}

		var options = new Options(version, strict, false, true, null, true);
		String lastExport = null;
		for (int r = 0; r < repeat; r++) {
			long t0 = System.nanoTime();
			AI ai;
			if (file != null) {
				LeekScript.setFileSystem(LeekScript.getNativeFileSystem());
				ai = LeekScript.compileFile(file, "AI", options);
			} else {
				ai = LeekScript.compileSnippet(code, "AI", options);
			}
			// Default {@link leekscript.runner.BasicAILog} prints system logs to {@code System.out}, which
			// would mix with the exported result string consumed by Rust benchmarks. Discard them here;
			// stderr is reserved for {@code leek_bench_iter} lines parsed by {@code leekscript-bench}.
			ai.getLogs().setStream(_a -> { /* parity: ignore soft runtime logs */ });
			long tCompileEnd = System.nanoTime();
			ai.init();
			ai.staticInit();
			// Keep parity aligned with the upstream JUnit harness: the operation counter should start
			// at 0 for snippet execution (e.g. `getOperations()` must return 0).
			ai.resetCounter();
			// Bench/parity snippets may intentionally be heavy (stress tests). Don't fail the run due
			// to the engine default ops budget.
			ai.setMaxOperations(Integer.MAX_VALUE);
			long tInitEnd = System.nanoTime();
			var v = ai.runIA();
			long tRunEnd = System.nanoTime();
			lastExport = ai.export(v, new HashSet<>());
			long tExportEnd = System.nanoTime();
			long ops = ai.operations();
			long exportHash = fnv1a64(lastExport.getBytes(StandardCharsets.UTF_8));
			String exportHashU = Long.toUnsignedString(exportHash);
			double compileMs = (tCompileEnd - t0) / 1_000_000.0;
			double initMs = (tInitEnd - tCompileEnd) / 1_000_000.0;
			double runMs = (tRunEnd - tInitEnd) / 1_000_000.0;
			double exportMs = (tExportEnd - tRunEnd) / 1_000_000.0;
			System.err.printf(
					Locale.US,
					"leek_bench_iter i=%d compile_ms=%.3f init_ms=%.3f run_ms=%.3f export_ms=%.3f ops=%d export_hash=%s%n",
					r,
					compileMs,
					initMs,
					runMs,
					exportMs,
					ops,
					exportHashU);
		}
		System.out.print(lastExport);
	}
}
