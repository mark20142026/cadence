// Decompile the functions that reference a defined string containing <text> into <outDir>.
// Usage: -postScript StringRefs.java <outDir> <text>
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.Reference;
import java.io.PrintWriter;
import java.nio.file.*;
import java.util.*;

public class StringRefs extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        Set<Function> done = new HashSet<>();
        for (Data data : currentProgram.getListing().getDefinedData(true)) {
            Object v = data.getValue();
            if (!(v instanceof String) || !((String) v).contains(args[1])) continue;
            for (Reference r : getReferencesTo(data.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                if (f == null || !done.add(f)) continue;
                DecompileResults res = d.decompileFunction(f, 180, monitor);
                try (PrintWriter w = new PrintWriter(out.resolve(f.getName() + "@" + f.getEntryPoint() + ".c").toFile())) {
                    w.println("// " + f.getName(true) + " (references \"" + v + "\" at " + r.getFromAddress() + ")");
                    w.println(res.decompileCompleted() ? res.getDecompiledFunction().getC() : "// failed");
                }
            }
        }
    }
}
