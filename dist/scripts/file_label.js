document.addEventListener("DOMContentLoaded", () => {
  let inputs = document.querySelectorAll('input#file');
  Array.prototype.forEach.call(inputs, function(input) {
    let label = input.nextElementSibling;
    let labelVal = label.innerHTML;
    input.addEventListener('change', function(e) {
      let fileName = '';

      if (this.files?.length > 1) {
        fileName = this.getAttribute('data-multiple-caption')?.replace('{count}', this.files.length);
      } else {
        fileName = e.target.value.split('\\').pop();
      }

      label.innerHTML = fileName || labelVal;
    });
  });
}, false);
