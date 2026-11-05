// Decompile the functions containing the given addresses (hex) into <out dir>.
// Usage: -postScript DecompileAt.java <outDir> <addr> [addr ...]
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import java.io.PrintWriter;
import java.nio.file.*;

public class DecompileAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (int i = 1; i < args.length; i++) {
            Function f = getFunctionContaining(toAddr(Long.parseUnsignedLong(args[i].replace("0x", ""), 16)));
            if (f == null) continue;
            DecompileResults r = d.decompileFunction(f, 120, monitor);
            try (PrintWriter w = new PrintWriter(out.resolve(f.getName() + "@" + f.getEntryPoint() + ".c").toFile())) {
                w.println("// " + f.getName(true) + " (contains " + args[i] + ")");
                w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed");
            }
        }
    }
}
