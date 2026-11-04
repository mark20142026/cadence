// List the callers of every function whose qualified name matches a regex.
// Usage: analyzeHeadless <proj> <name> -process -noanalysis -readOnly \
//          -postScript FindCallers.java <regex> <outFile>
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

import java.io.PrintWriter;
import java.util.regex.Pattern;

public class FindCallers extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Pattern pattern = Pattern.compile(args[0]);
        long base = currentProgram.getImageBase().getOffset();
        try (PrintWriter out = new PrintWriter(args[1])) {
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                String qname = f.getName(true);
                if (!pattern.matcher(qname).find()) continue;
                out.printf("%s 0x%x:%n", qname, f.getEntryPoint().getOffset() - base);
                for (Reference r : getReferencesTo(f.getEntryPoint())) {
                    Function c = getFunctionContaining(r.getFromAddress());
                    out.printf("  from 0x%x %s%n", r.getFromAddress().getOffset() - base,
                        c == null ? "?" : c.getName(true));
                }
            }
        }
    }
}
