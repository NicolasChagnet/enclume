// The title bar of a code block holds the name of the block and a button which
// copies its code. The button is written here, rather than while the site is
// built, so that a page which runs no script still reads the name alone.
//
// The stylesheet of the site (lumis.css) gives the button its look, and holds
// the two labels below.

const COPY_LABEL = "Copy";
const COPIED_LABEL = "Copied";

// The copy button of a block
function copyButton() {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "lumis-copy";
    button.dataset.copySnippet = "";
    button.textContent = COPY_LABEL;
    return button;
}

// Give a title bar its name and its button
function fillBar(bar) {
    // A page which writes the bar itself, or a script which runs twice, keeps
    // the button it already has
    if (bar.querySelector(".lumis-copy")) {
        return;
    }

    const name = document.createElement("span");
    name.className = "lumis-name";
    name.textContent = bar.textContent.trim();

    bar.replaceChildren(name, copyButton());
}

// The code of one line, without the number of the gutter
function lineText(line) {
    const code = line.cloneNode(true);
    for (const number of code.querySelectorAll(".l-line-number")) {
        number.remove();
    }
    return code.textContent ?? "";
}

// The code of a block, as its lines were written
function sourceOf(frame) {
    const lines = frame.querySelectorAll("pre.lumis [data-line]");
    return Array.from(lines, lineText).join("\n");
}

for (const bar of document.querySelectorAll(
    "figure.lumis-figure > figcaption.lumis-title",
)) {
    fillBar(bar);
}

document.addEventListener("click", async (event) => {
    const button = event.target.closest("[data-copy-snippet]");
    const frame = button?.closest("figure.lumis-figure");
    if (!frame) {
        return;
    }

    try {
        await navigator.clipboard.writeText(sourceOf(frame));
    } catch {
        // A browser which refuses the clipboard — an insecure origin, or a
        // reader who said no — keeps the label as it was
        return;
    }

    button.textContent = COPIED_LABEL;
    window.setTimeout(() => {
        button.textContent = COPY_LABEL;
    }, 1500);
});
