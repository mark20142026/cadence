// Decompile every function whose qualified name matches a regex, one file per function.
// Usage: analyzeHeadless <proj> <name> -process -noanalysis -readOnly \
//          -postScript DecompileMatching.java <regex> <outDir>
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;

import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.regex.Pattern;

public class DecompileFixed extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Pattern pattern = Pattern.compile(args[0]);
        Path outDir = Path.of(args[1]);
        Files.createDirectories(outDir);

        // Ghidra marks operator_new (and the stubs that call it) "no return", which cuts
        // functions off at their first allocation. Clear that in memory (the project is opened
        // read-only, nothing is saved).
        int fixed = 0;
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (f.hasNoReturn() && (f.getName().contains("operator_new") || f.getName().startsWith("FUN_") || f.getName().contains("Znwm"))) {
                f.setNoReturn(false);
                fixed++;
            }
        }
        for (Function f : currentProgram.getFunctionManager().getExternalFunctions()) {
            if (f.hasNoReturn() && f.getName().contains("operator_new")) {
                f.setNoReturn(false);
                fixed++;
            }
        }
        println("DecompileFixed: cleared noreturn on " + fixed + " functions");
        DecompInterface decomp = new DecompInterface();
        decomp.setOptions(new DecompileOptions());
        decomp.openProgram(currentProgram);
        long base = currentProgram.getImageBase().getOffset();
        int count = 0;
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (monitor.isCancelled()) break;
            String qname = f.getName(true);
            if (!pattern.matcher(qname).find()) continue;
            DecompileResults res = decomp.decompileFunction(f, 120, monitor);
            long off = f.getEntryPoint().getOffset() - base;
            String file = qname.replace("::", "__").replaceAll("[^\\w.~-]", "_") + String.format("@%x.c", off);
            try (PrintWriter out = new PrintWriter(outDir.resolve(file).toFile())) {
                out.printf("// %s  m1 0x%x%n", qname, off);
                out.println(res.decompileCompleted() ? res.getDecompiledFunction().getC()
                        : "// decompile failed: " + res.getErrorMessage());
            }
            count++;
        }
        decomp.dispose();
        println("DecompileFixed: wrote " + count + " functions to " + outDir);
    }
}
