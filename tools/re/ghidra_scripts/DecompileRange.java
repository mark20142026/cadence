// Decompile every function whose entry lies in [start, end) (hex) into <out dir>.
// Usage: -postScript DecompileRange.java <outDir> <start> <end>
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import java.io.PrintWriter;
import java.nio.file.*;

public class DecompileRange extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        Address a = toAddr(Long.parseUnsignedLong(args[1].replace("0x", ""), 16));
        Address b = toAddr(Long.parseUnsignedLong(args[2].replace("0x", ""), 16));
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (Function f : currentProgram.getFunctionManager().getFunctions(new AddressSet(a, b), true)) {
            DecompileResults r = d.decompileFunction(f, 120, monitor);
            try (PrintWriter w = new PrintWriter(out.resolve(f.getName() + "@" + f.getEntryPoint() + ".c").toFile())) {
                w.println("// " + f.getName(true));
                w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed");
            }
        }
    }
}
