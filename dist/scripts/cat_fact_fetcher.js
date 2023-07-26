fetch("https://catfact.ninja/fact")
  .then(data => data.json())
  .then(data => {
    document.getElementById("cat-fact").innerHTML = data.fact;
  });
