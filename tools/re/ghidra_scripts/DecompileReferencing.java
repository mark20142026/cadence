// Decompile every function that references a defined string containing <fragment>.
// Usage: analyzeHeadless <proj> <name> -process -noanalysis -readOnly \
//          -postScript DecompileReferencing.java <fragment> <outDir>
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.data.StringDataInstance;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileReferencing extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String fragment = args[0];
        Path outDir = Path.of(args[1]);
        Files.createDirectories(outDir);
        Set<Function> functions = new LinkedHashSet<>();
        for (Data d : currentProgram.getListing().getDefinedData(true)) {
            if (!d.hasStringValue()) continue;
            String value = StringDataInstance.getStringDataInstance(d).getStringValue();
            if (value == null || !value.contains(fragment)) continue;
            for (Reference r : getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                if (f != null) functions.add(f);
            }
        }
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        long base = currentProgram.getImageBase().getOffset();
        for (Function f : functions) {
            DecompileResults res = di.decompileFunction(f, 120, monitor);
            if (!res.decompileCompleted()) continue;
            String name = f.getName(true).replace("::", "__").replaceAll("[^A-Za-z0-9_]", "_");
            long off = f.getEntryPoint().getOffset() - base;
            Files.writeString(outDir.resolve(name + "@" + Long.toHexString(off) + ".c"),
                "// " + f.getName(true) + "  m1 0x" + Long.toHexString(off) + "\n\n" + res.getDecompiledFunction().getC());
        }
        println("DecompileReferencing: " + functions.size() + " functions reference '" + fragment + "'");
    }
}
