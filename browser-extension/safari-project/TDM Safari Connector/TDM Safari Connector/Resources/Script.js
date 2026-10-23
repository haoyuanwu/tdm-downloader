function show(enabled, useSettingsInsteadOfPreferences) {
    if (typeof enabled === "boolean") {
        document.body.classList.toggle(`state-on`, enabled);
        document.body.classList.toggle(`state-off`, !enabled);
    } else {
        document.body.classList.remove(`state-on`);
        document.body.classList.remove(`state-off`);
    }
}

function openPreferences() {
    webkit.messageHandlers.controller.postMessage("open-preferences");
}

document.querySelector("button.open-preferences").addEventListener("click", openPreferences);

function showHelp(message) {
    const help = document.querySelector('#setup-help');
    help.textContent = message;
    help.hidden = !message;
    if (message) document.querySelector('.unsigned-help').open = true;
}
document.querySelector('button.open-safari').addEventListener('click', () => webkit.messageHandlers.controller.postMessage('open-safari'));
document.querySelector('button.refresh-state').addEventListener('click', () => webkit.messageHandlers.controller.postMessage('refresh-state'));
