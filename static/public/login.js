const passwordBox = document.getElementById("password-box");

function clearError() {
    passwordBox.classList.remove("wrong");
}

async function submit() {
    let msg = token + ":" + passwordBox.value;
    let hash = forge_sha256(msg);
    let response = await fetch("/login", {
        method: "POST",
        body: JSON.stringify({ token, hash }),
    });
    if (response.redirected) {
        window.location.assign(response.url);
    } else {
        passwordBox.classList.add("wrong");
    }
}

addEventListener("keydown", function(evt) {
    if (evt.key == "Enter") {
        submit();
    }
})
