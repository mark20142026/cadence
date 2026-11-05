// Dump the data around each reference to a defined string equal to <text>: 8-byte words from
// <before> bytes before to <after> bytes after, each shown as a pointer to a string or a number.
// Usage: -postScript DataAround.java <text> <before> <after> <out>
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.Reference;
import java.io.PrintWriter;

public class DataAround extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        int before = Integer.parseInt(a[1]), after = Integer.parseInt(a[2]);
        try (PrintWriter w = new PrintWriter(a[3])) {
            for (Data data : currentProgram.getListing().getDefinedData(true)) {
                Object v = data.getValue();
                if (!(v instanceof String) || !v.equals(a[0])) continue;
                for (Reference r : getReferencesTo(data.getAddress())) {
                    Address at = r.getFromAddress();
                    w.println("ref from " + at + (getFunctionContaining(at) != null ? " (code)" : " (data)"));
                    for (long off = -before; off <= after; off += 4) {
                        Address x = at.add(off);
                        int word = getInt(x);
                        String s = "";
                        try {
                            Address p = toAddr(getLong(x));
                            Data d = getDataAt(p);
                            if (d != null && d.getValue() instanceof String) s = " -> \"" + d.getValue() + "\"";
                        } catch (Exception e) {}
                        w.println("  " + x + " " + word + s);
                    }
                }
            }
        }
    }
}
