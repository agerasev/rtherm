const main = () => {
    subscribe();
}

window.onload = main;

export const render = (sensors) => {
    const root = document.getElementById("sensors");

    let text = "";
    for (const name in sensors) {
        const sensor = sensors[name]
        const date = seconds_to_date(sensor.time);
        text += `<div>${name} (${format_date(date)}): <b>${sensor.value}</b> °C</div>`
    }
    if (text.length === 0) {
        text = "<i>No sensors</i>"
    }

    root.innerHTML = text;
}

const TIMEOUT = 10 * 1000;

const subscribe = () => {
    const xhttp = new XMLHttpRequest();
    xhttp.onload = function () {
        render(JSON.parse(this.responseText));
        setTimeout(subscribe, TIMEOUT);
    }
    xhttp.onerror = (e) => {
        console.error("Get error:", e);
        setTimeout(subscribe, TIMEOUT);
    };
    xhttp.open("GET", "/sensors", true);
    xhttp.send();
}

export const seconds_to_date = (seconds) => {
    let date = new Date(0);
    date.setUTCSeconds(seconds);
    return date;
}

export const format_date = (date) => {
    return ("0" + date.getDate()).slice(-2) + "-"
        + ("0" + (date.getMonth() + 1)).slice(-2) + "-"
        + date.getFullYear() + " "
        + ("0" + date.getHours()).slice(-2) + ":"
        + ("0" + date.getMinutes()).slice(-2);
}
