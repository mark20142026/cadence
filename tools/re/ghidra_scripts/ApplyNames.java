// Pre-analysis script: create and name functions from a names TSV ("0xOFFSET<TAB>Class::method").
// Offsets are relative to the image base (Geode's m1 addresses).
// Usage: analyzeHeadless ... -preScript ApplyNames.java <names.tsv>
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Namespace;
import ghidra.program.model.symbol.SourceType;

import java.nio.file.Files;
import java.nio.file.Path;

public class ApplyNames extends GhidraScript {
    @Override
    public void run() throws Exception {
        Path tsv = Path.of(getScriptArgs()[0]);
        Address base = currentProgram.getImageBase();
        int named = 0, failed = 0;
        for (String line : Files.readAllLines(tsv)) {
            String[] parts = line.split("\t");
            if (parts.length != 2) continue;
            Address addr = base.add(Long.decode(parts[0]));
            String full = parts[1];
            int sep = full.lastIndexOf("::");
            String name = sep < 0 ? full : full.substring(sep + 2);
            try {
                Namespace ns = currentProgram.getGlobalNamespace();
                if (sep >= 0) {
                    for (String part : full.substring(0, sep).split("::")) {
                        Namespace next = getNamespace(ns, part);
                        ns = next != null ? next
                                : currentProgram.getSymbolTable().createNameSpace(ns, part, SourceType.IMPORTED);
                    }
                }
                Function f = getFunctionAt(addr);
                if (f == null) {
                    new CreateFunctionCmd(addr).applyTo(currentProgram, monitor);
                    f = getFunctionAt(addr);
                }
                if (f == null) { failed++; continue; }
                f.setParentNamespace(ns);
                f.setName(name, SourceType.IMPORTED);
                named++;
            } catch (Exception e) {
                failed++;
            }
        }
        println("ApplyNames: named " + named + ", failed " + failed);
    }
}
