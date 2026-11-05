// List symbols whose name matches a regex, with the functions that reference them.
// Usage: -postScript SymbolRefs.java <regex> <out>
import ghidra.app.script.GhidraScript;
import ghidra.program.model.symbol.*;
import ghidra.program.model.listing.Function;
import java.io.PrintWriter;
import java.util.regex.Pattern;

public class SymbolRefs extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Pattern p = Pattern.compile(args[0]);
        try (PrintWriter out = new PrintWriter(args[1])) {
            SymbolIterator it = currentProgram.getSymbolTable().getAllSymbols(true);
            while (it.hasNext()) {
                Symbol s = it.next();
                if (!p.matcher(s.getName(true)).find()) continue;
                out.println(s.getName(true) + " @ " + s.getAddress());
                for (Reference r : getReferencesTo(s.getAddress())) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    out.println("    from " + r.getFromAddress() + " " + (f == null ? "?" : f.getName(true) + " @ " + f.getEntryPoint()));
                }
            }
        }
    }
}
