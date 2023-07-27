document.addEventListener("DOMContentLoaded", () => {
  const params = new Proxy(new URLSearchParams(window.location.search), {
    get: (searchParams, prop) => searchParams.get(prop),
  });


  if (params.link !== null) {
    let link = `${window.location.origin}/download/${params.link}`;

    let link_el = document.getElementById("link");

    link_el.href = link;
    link_el.innerHTML = link;
  }
});

function clipboard() {
  let copyText = document.getElementById("link");

  navigator.clipboard?.writeText(copyText.href).then(() => alert("Copied: " + copyText.href));
}
