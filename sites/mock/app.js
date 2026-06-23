const clock = document.querySelector("#clock");

function renderClock() {
  if (!clock) {
    return;
  }

  clock.textContent = new Intl.DateTimeFormat("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date());
}

renderClock();
setInterval(renderClock, 1000);
