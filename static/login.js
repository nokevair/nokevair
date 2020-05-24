const passwordBox = document.getElementById("password-box");

function clearError() {
    passwordBox.classList.remove("wrong");
}

function submit() {
    let msg = token + ":" + passwordBox.value;
    let hash = forge_sha256(msg);
    fetch("/login", {
        method: "POST",
        body: JSON.stringify({ token, hash }),
    }).then(response => {
        if (response.redirected) {
            window.location.assign(response.url);
        } else {
            passwordBox.classList.add("wrong");
        }
    })
}

addEventListener("keydown", function(evt) {
    if (evt.key == "Enter") {
        submit();
    }
})
