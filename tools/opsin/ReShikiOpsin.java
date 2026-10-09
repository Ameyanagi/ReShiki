// Original ReShiki adapter: MIT OR Apache-2.0. OPSIN retains its MIT license.
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import uk.ac.cam.ch.wwmm.opsin.NameToStructure;
import uk.ac.cam.ch.wwmm.opsin.NameToStructureConfig;
import uk.ac.cam.ch.wwmm.opsin.OpsinResult;
import uk.ac.cam.ch.wwmm.opsin.OpsinWarning;
import uk.ac.cam.ch.wwmm.opsin.SmilesOptions;

/** One bounded UTF-8 name on stdin; one structured result on stdout. No network. */
public final class ReShikiOpsin {
    private static final int INPUT_LIMIT = 2048;
    private static final int SMILES_FLAGS = SmilesOptions.CXSMILES_ENHANCED_STEREO
        | SmilesOptions.CXSMILES_POLYMERS | SmilesOptions.CXSMILES_ATOM_LABELS;

    private static String quote(String text) {
        if (text == null) return "null";
        StringBuilder out = new StringBuilder("\"");
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c == '\\' || c == '\"') out.append('\\').append(c);
            else if (c < 0x20) out.append(String.format("\\u%04x", (int)c));
            else out.append(c);
        }
        return out.append('\"').toString();
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 0 || !"2.9.0".equals(NameToStructure.getVersion())) {
            throw new IllegalStateException("Incompatible pinned OPSIN adapter");
        }
        ByteArrayOutputStream input = new ByteArrayOutputStream();
        byte[] buffer = new byte[256];
        for (int count; (count = System.in.read(buffer)) != -1;) {
            if (input.size() + count > INPUT_LIMIT) throw new IllegalArgumentException("Name exceeds limit");
            input.write(buffer, 0, count);
        }
        String name = StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(input.toByteArray())).toString();
        if (name.isEmpty() || name.chars().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException("Invalid name input");
        }
        NameToStructureConfig config = new NameToStructureConfig();
        config.setAllowRadicals(false);
        config.setOutputRadicalsAsWildCardAtoms(false);
        config.setDetailedFailureAnalysis(false);
        config.setInterpretAcidsWithoutTheWordAcid(false);
        config.setWarnRatherThanFailOnUninterpretableStereochemistry(false);
        OpsinResult result = NameToStructure.getInstance().parseChemicalName(name, config);
        StringBuilder out = new StringBuilder("{\"protocol\":1,\"version\":\"2.9.0\",\"options\":\"strict-cx13\",\"name\":");
        out.append(quote(name)).append(",\"status\":").append(quote(result.getStatus().toString()))
            .append(",\"message\":").append(quote(result.getMessage())).append(",\"warnings\":[");
        boolean first = true;
        for (OpsinWarning warning : result.getWarnings()) {
            if (!first) out.append(',');
            first = false;
            out.append("{\"kind\":").append(quote(warning.getType().toString()))
                .append(",\"message\":").append(quote(warning.getMessage())).append('}');
        }
        out.append("],\"cxsmiles\":").append(quote(result.getSmiles(SMILES_FLAGS))).append('}');
        byte[] response = out.toString().getBytes(StandardCharsets.UTF_8);
        if (response.length > 65536) throw new IllegalStateException("Parser output exceeds limit");
        System.out.write(response);
        System.out.flush();
    }
}
